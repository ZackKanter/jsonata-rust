use json::JsonValue;
use std::collections::HashMap;

use crate::error::*;
use crate::functions::*;
use crate::parser::ast::*;
use crate::JsonAtaResult;

use super::frame::{Binding, Frame};
pub use super::value::Value;

pub fn evaluate(node: &Node, input: &Value, frame: &mut Frame) -> JsonAtaResult<Value> {
    let mut result = match &node.kind {
        NodeKind::Path => evaluate_path(node, input, frame)?,
        NodeKind::Binary(_) => evaluate_binary_op(node, input, frame)?,
        NodeKind::Unary(_) => evaluate_unary_op(node, input, frame)?,
        NodeKind::Name(_) => evaluate_name(node, input)?,
        NodeKind::Null => Value::Raw(JsonValue::Null),
        NodeKind::Bool(value) => Value::Raw(json::from(*value)),
        NodeKind::Str(value) => Value::Raw(json::from(value.clone())),
        NodeKind::Num(value) => Value::Raw(json::from(*value)),
        NodeKind::Ternary => evaluate_ternary(node, input, frame)?,
        NodeKind::Block => evaluate_block(node, input, frame)?,
        NodeKind::Var(name) => evaluate_variable(name, input, frame)?,
        NodeKind::Wildcard => evaluate_wildcard(input)?,
        NodeKind::Descendent => evaluate_descendents(input)?,
        // TODO:
        //  - Descendant
        //  - Parent
        //  - Regex
        //  - Function
        //  - Lambda
        //  - Partial
        //  - Apply
        //  - Transform
        _ => unimplemented!("TODO: node kind not yet supported: {}", node.kind),
    };

    if let Some(ref predicate) = node.predicate {
        result = evaluate_filter(predicate, &result, frame)?;
    }

    match &node.group_by {
        Some(object) if !node.is_path() => {
            result = evaluate_group_expression(node, object, &result, frame)?;
        }
        _ => {}
    }

    if result.is_seq() {
        if node.keep_array {
            result.set_keep_array();
        }
        if result.len() == 0 {
            Ok(Value::Undefined)
        } else if result.len() == 1 {
            if result.keep_array() {
                Ok(result)
            } else {
                Ok(result.as_array_mut().swap_remove(0))
            }
        } else {
            Ok(result)
        }
    } else {
        Ok(result)
    }
}

fn evaluate_name(node: &Node, input: &Value) -> JsonAtaResult<Value> {
    if let NodeKind::Name(key) = &node.kind {
        Ok(lookup(input, key))
    } else {
        unreachable!()
    }
}

fn evaluate_unary_op(node: &Node, input: &Value, frame: &mut Frame) -> JsonAtaResult<Value> {
    if let NodeKind::Unary(op) = &node.kind {
        match op {
            UnaryOp::Minus => {
                let result = evaluate(&node.children[0], input, frame)?;
                if let Some(num) = result.as_f64() {
                    Ok(Value::Raw((-num).into()))
                } else {
                    Err(Box::new(D1002 {
                        position: node.position,
                        value: result.as_raw().to_string(),
                    }))
                }
            }
            UnaryOp::ArrayConstructor => {
                let mut result = Value::new_array();
                for child in &node.children {
                    let value = evaluate(child, input, frame)?;
                    if !value.is_undef() {
                        if let NodeKind::Unary(UnaryOp::ArrayConstructor) = child.kind {
                            result.push(value)
                        } else {
                            result = append(result, value);
                        }
                    }
                }
                if node.keep_array {
                    result.set_keep_array();
                }
                Ok(result)
            }
            UnaryOp::ObjectConstructor(object) => {
                evaluate_group_expression(node, object, input, frame)
            }
        }
    } else {
        panic!("`node` should be a NodeKind::Unary");
    }
}

fn evaluate_group_expression(
    node: &Node,
    object: &Object,
    input: &Value,
    frame: &mut Frame,
) -> JsonAtaResult<Value> {
    // TODO: This code is horrible

    let input = if input.is_array() {
        input.clone()
    } else {
        Value::new_seq_from(input)
    };

    let mut groups: HashMap<String, (Value, usize)> = HashMap::new();

    for input in input.iter() {
        for (i, (k, _)) in object.iter().enumerate() {
            let key = evaluate(k, input, frame)?.as_string();

            if key.is_none() {
                return Err(box T1003 {
                    position: node.position,
                    value: k.to_string(),
                });
            }

            let key = key.unwrap();

            if groups.contains_key(&key) {
                if groups[&key].1 != i {
                    return Err(box D1009 {
                        position: node.position,
                        value: k.to_string(),
                    });
                }

                groups.insert(
                    key.clone(),
                    (append(groups[&key].0.clone(), input.clone()), i),
                );
            } else {
                groups.insert(key, (input.clone(), i));
            }
        }
    }

    let mut result = JsonValue::Object(json::object::Object::new());
    for key in groups.keys() {
        let value = evaluate(&object[groups[key].1].1, &groups[key].0, frame)?;
        if !value.is_undef() {
            result.insert(key, value.into_raw()).unwrap();
        }
    }

    Ok(Value::Raw(result))
}

fn evaluate_binary_op(node: &Node, input: &Value, frame: &mut Frame) -> JsonAtaResult<Value> {
    use BinaryOp::*;
    if let NodeKind::Binary(op) = &node.kind {
        match op {
            Add | Subtract | Multiply | Divide | Modulus => {
                evaluate_numeric_expression(node, input, frame, op)
            }
            LessThan | LessThanEqual | GreaterThan | GreaterThanEqual => {
                evaluate_comparison_expression(node, input, frame, op)
            }
            Equal | NotEqual => evaluate_equality_expression(node, input, frame, op),
            Concat => evaluate_string_concat(node, input, frame),
            Bind => evaluate_bind_expression(node, input, frame),
            Or | And => evaluate_boolean_expression(node, input, frame, op),
            In => evaluate_includes_expression(node, input, frame),
            Range => evaluate_range_expression(node, input, frame),
            _ => unreachable!("Unexpected binary operator {:#?}", op),
        }
    } else {
        panic!("`node` should be a NodeKind::Binary")
    }
}

fn evaluate_bind_expression(node: &Node, input: &Value, frame: &mut Frame) -> JsonAtaResult<Value> {
    let name = &node.children[0];
    let value = evaluate(&node.children[1], input, frame)?;

    if let NodeKind::Var(name) = &name.kind {
        frame.bind(name, Binding::Var(value.clone()));
    }

    Ok(value)
}

fn evaluate_numeric_expression(
    node: &Node,
    input: &Value,
    frame: &mut Frame,
    op: &BinaryOp,
) -> JsonAtaResult<Value> {
    let lhs = evaluate(&node.children[0], input, frame)?;
    let rhs = evaluate(&node.children[1], input, frame)?;

    let lhs: f64 = match lhs.as_raw() {
        JsonValue::Number(value) => value.clone().into(),
        _ => {
            return Err(Box::new(T2001 {
                position: node.position,
                op: op.to_string(),
            }))
        }
    };

    let rhs: f64 = match rhs.as_raw() {
        JsonValue::Number(value) => value.clone().into(),
        _ => {
            return Err(Box::new(T2002 {
                position: node.position,
                op: op.to_string(),
            }))
        }
    };

    let result = match op {
        BinaryOp::Add => lhs + rhs,
        BinaryOp::Subtract => lhs - rhs,
        BinaryOp::Multiply => lhs * rhs,
        BinaryOp::Divide => lhs / rhs,
        BinaryOp::Modulus => lhs % rhs,
        _ => unreachable!(),
    };

    Ok(Value::Raw(result.into()))
}

fn evaluate_comparison_expression(
    node: &Node,
    input: &Value,
    frame: &mut Frame,
    op: &BinaryOp,
) -> JsonAtaResult<Value> {
    let lhs = evaluate(&node.children[0], input, frame)?;
    let rhs = evaluate(&node.children[1], input, frame)?;

    let lhs = match lhs {
        Value::Undefined => return Ok(Value::Undefined),
        _ => lhs.as_raw(),
    };

    let rhs = match rhs {
        Value::Undefined => return Ok(Value::Undefined),
        _ => rhs.as_raw(),
    };

    if !((lhs.is_number() || lhs.is_string()) && (rhs.is_number() || rhs.is_string())) {
        return Err(Box::new(T2010 {
            position: node.position,
            op: op.to_string(),
        }));
    }

    if lhs.is_number() && rhs.is_number() {
        let lhs = lhs.as_f64().unwrap();
        let rhs = rhs.as_f64().unwrap();

        return Ok(Value::Raw(json::from(match op {
            BinaryOp::LessThan => lhs < rhs,
            BinaryOp::LessThanEqual => lhs <= rhs,
            BinaryOp::GreaterThan => lhs > rhs,
            BinaryOp::GreaterThanEqual => lhs >= rhs,
            _ => unreachable!(),
        })));
    }

    if lhs.is_string() && rhs.is_string() {
        let lhs = lhs.as_str().unwrap();
        let rhs = rhs.as_str().unwrap();

        return Ok(Value::Raw(json::from(match op {
            BinaryOp::LessThan => lhs < rhs,
            BinaryOp::LessThanEqual => lhs <= rhs,
            BinaryOp::GreaterThan => lhs > rhs,
            BinaryOp::GreaterThanEqual => lhs >= rhs,
            _ => unreachable!(),
        })));
    }

    Err(Box::new(T2009 {
        position: node.position,
        lhs: lhs.to_string(),
        rhs: rhs.to_string(),
        op: op.to_string(),
    }))
}

fn evaluate_boolean_expression(
    node: &Node,
    input: &Value,
    frame: &mut Frame,
    op: &BinaryOp,
) -> JsonAtaResult<Value> {
    let lhs = evaluate(&node.children[0], input, frame)?;
    let rhs = evaluate(&node.children[1], input, frame)?;

    let left_bool = boolean(&lhs);
    let right_bool = boolean(&rhs);

    let result = match op {
        BinaryOp::And => left_bool && right_bool,
        BinaryOp::Or => left_bool || right_bool,
        _ => unreachable!(),
    };

    Ok(Value::Raw(result.into()))
}

fn evaluate_includes_expression(
    node: &Node,
    input: &Value,
    frame: &mut Frame,
) -> JsonAtaResult<Value> {
    let lhs = evaluate(&node.children[0], input, frame)?;
    let rhs = evaluate(&node.children[1], input, frame)?;

    if !rhs.is_array() {
        return Ok(Value::Raw((lhs.as_raw() == rhs.as_raw()).into()));
    }

    for item in rhs.iter() {
        if item.is_raw() && lhs.as_raw() == item.as_raw() {
            return Ok(Value::Raw(true.into()));
        }
    }

    return Ok(Value::Raw(false.into()));
}

fn evaluate_equality_expression(
    node: &Node,
    input: &Value,
    frame: &mut Frame,
    op: &BinaryOp,
) -> JsonAtaResult<Value> {
    let lhs = evaluate(&node.children[0], input, frame)?;
    let rhs = evaluate(&node.children[1], input, frame)?;

    if lhs.is_undef() || rhs.is_undef() {
        return Ok(Value::Raw(false.into()));
    }

    let result = match op {
        BinaryOp::Equal => lhs == rhs,
        BinaryOp::NotEqual => lhs != rhs,
        _ => unreachable!(),
    };

    Ok(Value::Raw(result.into()))
}

fn evaluate_string_concat(node: &Node, input: &Value, frame: &mut Frame) -> JsonAtaResult<Value> {
    let lhs = evaluate(&node.children[0], input, frame)?;
    let rhs = evaluate(&node.children[1], input, frame)?;

    let mut lstr = string(lhs).unwrap();
    let rstr = string(rhs).unwrap();

    lstr.push_str(&rstr);

    Ok(Value::Raw(lstr.into()))
}

fn evaluate_range_expression(
    node: &Node,
    input: &Value,
    frame: &mut Frame,
) -> JsonAtaResult<Value> {
    let lhs = evaluate(&node.children[0], input, frame)?;
    let rhs = evaluate(&node.children[1], input, frame)?;

    if lhs.is_undef() || rhs.is_undef() {
        return Ok(Value::Undefined);
    }

    let lhs = match lhs.as_usize() {
        Some(num) => num,
        None => {
            return Err(box T2003 {
                position: node.position,
            })
        }
    };

    let rhs = match rhs.as_usize() {
        Some(num) => num,
        None => {
            return Err(box T2004 {
                position: node.position,
            })
        }
    };

    if lhs > rhs {
        return Ok(Value::Undefined);
    }

    let size = rhs - lhs + 1;
    if size > 10_000_000_000 {
        return Err(box D2014 {
            position: node.position,
            value: size.to_string(),
        });
    }

    let mut result = Value::new_seq_with_capacity(size);
    for i in lhs..rhs + 1 {
        result.push(Value::Raw(i.into()))
    }

    Ok(result)
}

fn evaluate_path(node: &Node, input: &Value, frame: &mut Frame) -> JsonAtaResult<Value> {
    let mut input = if input.is_array() && !matches!(&node.children[0].kind, NodeKind::Var(_)) {
        input.clone()
    } else {
        Value::new_seq_from(input)
    };

    let mut result = Value::Undefined;

    for (step_index, step) in node.children.iter().enumerate() {
        result = evaluate_step(step, &input, frame, step_index == node.children.len() - 1)?;

        match result {
            Value::Undefined => break,
            Value::Raw(..) => panic!("unexpected Value::Raw"),
            Value::Array { .. } => {
                if result.is_empty() {
                    break;
                }

                input = result.clone();
            }
        }
    }

    // TODO: Tuple, singleton array (jsonata.js:164)

    match &node.group_by {
        Some(object) if !node.is_path() => {
            result = evaluate_group_expression(node, object, &result, frame)?;
        }
        _ => {}
    }

    Ok(result)
}

fn evaluate_step(
    node: &Node,
    input: &Value,
    frame: &mut Frame,
    last_step: bool,
) -> JsonAtaResult<Value> {
    let mut result = Value::new_seq();

    // if let NodeKind::Sort = node.kind {
    //     result = evaluate_sort_expression(node, input, frame);
    //     if node.stages.is_some() {
    //       result = evaluate_stages(node.stages, &result, frame)?;
    //     }
    // }

    for input in input.iter() {
        let mut res = evaluate(node, input, frame)?;

        if let Some(ref stages) = node.stages {
            for stage in stages {
                res = evaluate_filter(stage, &res, frame)?;
            }
        }

        if !res.is_undef() {
            result.push(res);
        }
    }

    if last_step && result.len() == 1 && result[0].is_array() && !result[0].is_seq() {
        Ok(result[0].clone())
    } else {
        // Flatten the result
        let mut flattened = Value::new_seq();
        result.iter().cloned().for_each(|v| {
            if !v.is_array() || v.keep_array() {
                flattened.push(v.clone())
            } else {
                v.iter().cloned().for_each(|v| flattened.push(v.clone()))
            }
        });
        Ok(flattened)
    }
}

// fn evaluate_sort_expression(node: &Node, input: &Value, frame: &mut Frame) {

// }

// fn evaluate_stages(stages: Option<&Vec<Node>>, input: &Value, frame: &mut Frame) {

// }

fn evaluate_block(node: &Node, input: &Value, frame: &mut Frame) -> JsonAtaResult<Value> {
    if let NodeKind::Block = &node.kind {
        let mut frame = Frame::new_with_parent(frame);
        let mut result = Value::Undefined;

        for child in &node.children {
            result = evaluate(child, input, &mut frame)?;
        }

        Ok(result)
    } else {
        panic!("`node` should be a NodeKind::Block");
    }
}

fn evaluate_ternary(node: &Node, input: &Value, frame: &mut Frame) -> JsonAtaResult<Value> {
    if let NodeKind::Ternary = &node.kind {
        let condition = evaluate(&node.children[0], input, frame)?;
        if boolean(&condition) {
            evaluate(&node.children[1], input, frame)
        } else if node.children.len() > 2 {
            evaluate(&node.children[2], input, frame)
        } else {
            Ok(Value::Undefined)
        }
    } else {
        panic!("`node` should be a NodeKind::Ternary")
    }
}

fn evaluate_variable(name: &str, input: &Value, frame: &Frame) -> JsonAtaResult<Value> {
    if name == "" {
        // Empty variable name returns the context value
        if input.is_wrapped() {
            Ok(input.clone().unwrap())
        } else {
            Ok(input.clone())
        }
    } else {
        if let Some(binding) = frame.lookup(name) {
            Ok(binding.as_var().clone())
        } else {
            Ok(Value::Undefined)
        }
    }
}

fn evaluate_filter(node: &Node, input: &Value, frame: &mut Frame) -> JsonAtaResult<Value> {
    let mut results = Value::new_seq();

    let input = if input.is_array() {
        input.clone()
    } else {
        Value::new_seq_from(input)
    };

    if let NodeKind::Num(num) = node.kind {
        let index = if num < 0. {
            (num.floor() as isize).wrapping_add(input.len() as isize) as usize
        } else {
            num.floor() as usize
        };

        if index < input.len() {
            let item = &input[index as usize];
            if !item.is_undef() {
                if item.is_array() {
                    results = item.clone();
                } else {
                    results.push(item.clone());
                }
            }
        }
    } else {
        for (index, item) in input.iter().enumerate() {
            let res = evaluate(node, item, frame)?;

            let indices = if let Some(num) = res.as_f64() {
                vec![num]
            } else if let Some(indices) = res.as_f64_vec() {
                indices
            } else {
                vec![]
            };

            if !indices.is_empty() {
                indices.iter().for_each(|num| {
                    let ii = if *num < 0. {
                        (num.floor() as isize).wrapping_add(input.len() as isize) as usize
                    } else {
                        num.floor() as usize
                    };
                    if ii == index {
                        results.push(item.clone());
                    }
                });
            } else if boolean(&res) {
                results.push(item.clone());
            }
        }
    }

    Ok(results)
}

fn evaluate_wildcard(input: &Value) -> JsonAtaResult<Value> {
    let mut result = Value::new_seq();

    fn flatten(value: &Value, result: &mut Value) {
        if value.is_array() {
            value.iter().for_each(|value| {
                flatten(value, result);
            });
        } else {
            result.push(value.clone());
        }
    }

    if input.is_object() {
        for (_key, value) in input.as_raw().entries() {
            let value = Value::new(Some(value));
            if value.is_array() {
                flatten(&value, &mut result);
            } else {
                result.push(value);
            }
        }
    }

    Ok(result)
}

fn evaluate_descendents(input: &Value) -> JsonAtaResult<Value> {
    let mut result = Value::Undefined;
    let mut result_seq = Value::new_seq();

    fn recurse(value: &Value, result: &mut Value) {
        if !value.is_array() {
            result.push(value.clone());
        }
        if value.is_array() {
            value.iter().for_each(|value| recurse(value, result));
        } else if value.is_object() {
            for (_key, value) in value.as_raw().entries() {
                let value = Value::new(Some(value));
                recurse(&value, result);
            }
        }
    }

    if !input.is_undef() {
        recurse(input, &mut result_seq);
        if result_seq.len() == 1 {
            result = result_seq.as_array_mut().swap_remove(0);
        } else {
            result = result_seq;
        }
    }

    Ok(result)
}