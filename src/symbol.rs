use super::ast::*;
use super::parser::Parser;
use super::tokenizer::{Token, TokenKind};
use super::{Error, Result};

pub(super) trait Symbol {
    fn lbp(&self) -> u32;
    fn nud(&self, parser: &mut Parser) -> Result<Node>;
    fn led(&self, parser: &mut Parser, left: Node) -> Result<Node>;
}

impl Symbol for Token {
    fn lbp(&self) -> u32 {
        use TokenKind::*;
        match &self.kind {
            End | Range | Colon | Comma | SemiColon | RightParen | RightBracket | RightBrace
            | Pipe | Not | Tilde | Null | Bool(..) | Str(..) | Num(..) | Name(..) | Var(..) => 0,
            Bind => 10,
            Question => 20,
            Or => 25,
            And => 30,
            NotEqual | GreaterEqual | LessEqual | Apply | In | Equal | RightCaret | LeftCaret
            | Caret => 40,
            Ampersand | Plus | Minus => 50,
            Wildcard | Descendent | ForwardSlash | Percent => 60,
            LeftBrace => 70,
            Period => 75,
            LeftBracket | LeftParen => 80,
            At | Hash => 80,
        }
    }

    fn nud(&self, parser: &mut Parser) -> Result<Node> {
        match self.kind {
            TokenKind::Null => Ok(Node::new(NodeKind::Null, self.position)),
            TokenKind::Bool(ref v) => Ok(Node::new(NodeKind::Bool(*v), self.position)),
            TokenKind::Str(ref v) => Ok(Node::new(NodeKind::String(v.clone()), self.position)),
            TokenKind::Num(ref v) => Ok(Node::new(NodeKind::Number(*v), self.position)),
            TokenKind::Name(ref v) => Ok(Node::new(NodeKind::Name(v.clone()), self.position)),
            TokenKind::Var(ref v) => Ok(Node::new(NodeKind::Var(v.clone()), self.position)),
            TokenKind::And => Ok(Node::new(
                NodeKind::Name(String::from("and")),
                self.position,
            )),
            TokenKind::Or => Ok(Node::new(NodeKind::Name(String::from("or")), self.position)),
            TokenKind::In => Ok(Node::new(NodeKind::Name(String::from("in")), self.position)),
            TokenKind::Minus => Ok(Node::new(
                NodeKind::Unary(UnaryOp::Minus(Box::new(parser.expression(70)?))),
                self.position,
            )),
            TokenKind::Wildcard => Ok(Node::new(NodeKind::Wildcard, self.position)),
            TokenKind::Descendent => Ok(Node::new(NodeKind::Descendent, self.position)),
            TokenKind::Percent => Ok(Node::new(NodeKind::Parent, self.position)),

            // Block of expressions
            TokenKind::LeftParen => {
                let mut expressions = Vec::new();

                while parser.token().kind != TokenKind::RightParen {
                    expressions.push(parser.expression(0)?);
                    if parser.token().kind != TokenKind::SemiColon {
                        break;
                    }
                    parser.expect(TokenKind::SemiColon, false)?;
                }
                parser.expect(TokenKind::RightParen, true)?;

                Ok(Node::new(NodeKind::Block(expressions), self.position))
            }

            // Array constructor
            TokenKind::LeftBracket => {
                let mut expressions = Vec::new();

                if parser.token().kind != TokenKind::RightBracket {
                    loop {
                        let mut item = parser.expression(0)?;

                        if parser.token().kind == TokenKind::Range {
                            parser.expect(TokenKind::Range, false)?;
                            item = Node::new(
                                NodeKind::Binary(
                                    BinaryOp::Range,
                                    Box::new(item),
                                    Box::new(parser.expression(0)?),
                                ),
                                self.position,
                            )
                        }

                        expressions.push(item);

                        if parser.token().kind != TokenKind::Comma {
                            break;
                        }

                        parser.expect(TokenKind::Comma, false)?;
                    }
                }
                parser.expect(TokenKind::RightBracket, true)?;

                Ok(Node::new(
                    NodeKind::Unary(UnaryOp::ArrayConstructor(expressions)),
                    self.position,
                ))
            }

            // Object constructor
            TokenKind::LeftBrace => Ok(Node::new(
                NodeKind::Unary(UnaryOp::ObjectConstructor(parse_object(parser)?)),
                self.position,
            )),

            // Object transformer
            TokenKind::Pipe => {
                let pattern = Box::new(parser.expression(0)?);

                parser.expect(TokenKind::Pipe, false)?;

                let update = Box::new(parser.expression(0)?);

                let delete = if parser.token().kind == TokenKind::Comma {
                    parser.expect(TokenKind::Comma, false)?;
                    Some(Box::new(parser.expression(0)?))
                } else {
                    None
                };

                parser.expect(TokenKind::Pipe, false)?;

                Ok(Node::new(
                    NodeKind::Transform {
                        pattern,
                        update,
                        delete,
                    },
                    self.position,
                ))
            }

            _ => Err(Error::invalid_unary(self.position, &self.kind)),
        }
    }

    fn led(&self, parser: &mut Parser, mut left: Node) -> Result<Node> {
        macro_rules! binary {
            ($n:tt) => {
                Ok(Node::new(
                    NodeKind::Binary(
                        BinaryOp::$n,
                        Box::new(left),
                        Box::new(parser.expression(self.lbp())?),
                    ),
                    self.position,
                ))
            };
        }

        match self.kind {
            TokenKind::Period => binary!(Map),
            TokenKind::Plus => binary!(Add),
            TokenKind::Minus => binary!(Subtract),
            TokenKind::Wildcard => binary!(Multiply),
            TokenKind::ForwardSlash => binary!(Divide),
            TokenKind::Percent => binary!(Modulus),
            TokenKind::Equal => binary!(Equal),
            TokenKind::LeftCaret => binary!(LessThan),
            TokenKind::RightCaret => binary!(GreaterThan),
            TokenKind::NotEqual => binary!(NotEqual),
            TokenKind::LessEqual => binary!(LessThanEqual),
            TokenKind::GreaterEqual => binary!(GreaterThanEqual),
            TokenKind::Ampersand => binary!(Concat),
            TokenKind::And => binary!(And),
            TokenKind::Or => binary!(Or),
            TokenKind::In => binary!(In),
            TokenKind::Apply => binary!(Apply),

            // Function calls or lambda definitions
            TokenKind::LeftParen => {
                let mut args = Vec::new();
                let mut is_partial = false;
                let mut is_lambda = false;

                if parser.token().kind != TokenKind::RightParen {
                    loop {
                        match parser.token().kind {
                            TokenKind::Question => {
                                is_partial = true;
                                args.push(Node::new(NodeKind::PartialArg, parser.token().position));
                                parser.expect(TokenKind::Question, false)?;
                            }
                            _ => {
                                args.push(parser.expression(0)?);
                            }
                        }
                        if parser.token().kind != TokenKind::Comma {
                            break;
                        }
                        parser.expect(TokenKind::Comma, false)?;
                    }
                }
                parser.expect(TokenKind::RightParen, true)?;

                let name = match left.kind {
                    NodeKind::Name(ref name) => {
                        // If the name of the function is 'function' or λ, then this is a function definition (lambda function)
                        if name == "function" || name == "λ" {
                            is_lambda = true;

                            // All of the args must be Variable nodes
                            for arg in &args {
                                if !matches!(arg.kind, NodeKind::Var(..)) {
                                    return Err(Error::invalid_function_param(
                                        arg.position,
                                        &self.kind,
                                    ));
                                }
                            }

                            // TODO: Parse function signatures
                        }
                        name.clone()
                    }
                    NodeKind::Var(ref name) => name.clone(),
                    _ => unreachable!(),
                };

                let func: Node;

                if is_lambda {
                    parser.expect(TokenKind::LeftBrace, false)?;
                    let body = Box::new(parser.expression(0)?);
                    func = Node::new(NodeKind::Lambda { name, args, body }, self.position);
                    parser.expect(TokenKind::RightBrace, false)?;
                } else {
                    func = Node::new(
                        NodeKind::Function {
                            name,
                            proc: Box::new(left),
                            args,
                            is_partial,
                        },
                        self.position,
                    );
                }

                Ok(func)
            }

            // Variable assignment
            TokenKind::Bind => {
                if !matches!(left.kind, NodeKind::Var(..)) {
                    return Err(Error::ExpectedVarLeft(left.position));
                }

                Ok(Node::new(
                    NodeKind::Binary(
                        BinaryOp::Bind,
                        Box::new(left),
                        Box::new(parser.expression(self.lbp() - 1)?),
                    ),
                    self.position,
                ))
            }

            // Order by expression
            TokenKind::Caret => {
                let mut terms = Vec::new();

                parser.expect(TokenKind::LeftParen, false)?;
                loop {
                    let mut descending = false;
                    if parser.token().kind == TokenKind::LeftCaret {
                        parser.expect(TokenKind::LeftCaret, false)?;
                    } else if parser.token().kind == TokenKind::RightCaret {
                        parser.expect(TokenKind::RightCaret, false)?;
                        descending = true;
                    }

                    terms.push((parser.expression(0)?, descending));

                    if parser.token().kind != TokenKind::Comma {
                        break;
                    }
                    parser.expect(TokenKind::Comma, false)?;
                }
                parser.expect(TokenKind::RightParen, false)?;

                Ok(Node::new(
                    NodeKind::OrderBy(Box::new(left), terms),
                    self.position,
                ))
            }

            // Context variable bind
            TokenKind::At => {
                let rhs = parser.expression(self.lbp())?;

                if !matches!(rhs.kind, NodeKind::Var(..)) {
                    return Err(Error::expected_var_right(rhs.position, "@"));
                }

                Ok(Node::new(
                    NodeKind::Binary(BinaryOp::ContextBind, Box::new(left), Box::new(rhs)),
                    self.position,
                ))
            }

            // Positional variable bind
            TokenKind::Hash => {
                let rhs = parser.expression(self.lbp())?;

                if !matches!(rhs.kind, NodeKind::Var(..)) {
                    return Err(Error::expected_var_right(rhs.position, "#"));
                }

                Ok(Node::new(
                    NodeKind::Binary(BinaryOp::PositionalBind, Box::new(left), Box::new(rhs)),
                    self.position,
                ))
            }

            // Ternary conditional
            TokenKind::Question => {
                let truthy = Box::new(parser.expression(0)?);

                let falsy = if parser.token().kind == TokenKind::Colon {
                    parser.expect(TokenKind::Colon, false)?;
                    Some(Box::new(parser.expression(0)?))
                } else {
                    None
                };

                Ok(Node::new(
                    NodeKind::Ternary {
                        cond: Box::new(left),
                        truthy,
                        falsy,
                    },
                    self.position,
                ))
            }

            // Object group by
            TokenKind::LeftBrace => Ok(Node::new(
                NodeKind::GroupBy(Box::new(left), parse_object(parser)?),
                self.position,
            )),

            // Array predicate or index
            TokenKind::LeftBracket => {
                if parser.token().kind == TokenKind::RightBracket {
                    // Empty predicate means maintain singleton arrays in the output

                    let mut step = &mut left;

                    // Walk back through left hand sides to find something that's not an array
                    // predicate
                    while let NodeKind::Binary(BinaryOp::Predicate, ref mut left, ..) = step.kind {
                        step = left
                    }

                    step.keep_array = true;

                    parser.expect(TokenKind::RightBracket, false)?;

                    Ok(left)
                } else {
                    let rhs = parser.expression(0)?;
                    parser.expect(TokenKind::RightBracket, true)?;
                    Ok(Node::new(
                        NodeKind::Binary(BinaryOp::Predicate, Box::new(left), Box::new(rhs)),
                        self.position,
                    ))
                }
            }

            _ => Err(Error::syntax_error(self.position, &self.kind)),
        }
    }
}

/// Parses an object definition.
fn parse_object(parser: &mut Parser) -> Result<Object> {
    let mut object: Object = Vec::new();
    if parser.token().kind != TokenKind::RightBrace {
        loop {
            let key = parser.expression(0)?;
            parser.expect(TokenKind::Colon, false)?;
            let value = parser.expression(0)?;
            object.push((key, value));
            if parser.token().kind != TokenKind::Comma {
                break;
            }
            parser.expect(TokenKind::Comma, false)?;
        }
    }
    parser.expect(TokenKind::RightBrace, true)?;
    Ok(object)
}
