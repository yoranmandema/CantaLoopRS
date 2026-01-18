/// Lowering module: CST → AST
///
/// This module converts the Concrete Syntax Tree (with spans) to the
/// Abstract Syntax Tree (semantic, without spans).
///
/// The lowering process:
/// 1. Extracts semantic information from CST nodes
/// 2. Drops spans (unless explicitly needed)
/// 3. Produces clean AST for semantic analysis
use crate::core::ast::*;
use crate::core::cst::*;

/// Helper to create an AstIdent from a Spanned<String>.
///
/// Phase 3: Extracts name and CstId from CST identifier for identity tracking.
fn make_ast_ident(ident: &Spanned<String>) -> AstIdent {
    AstIdent {
        name: ident.node.clone(),
        cst_id: ident.id,
    }
}

/// Lower a CST program to an AST program.
/// Also returns a map of documentation blocks keyed by declaration identifier name (for LSP lookup).
/// The docs map uses identifier names as keys since spans won't be available during LSP queries.
pub fn lower_cst_to_ast(
    cst: CstProgram,
    cst_docs: std::collections::HashMap<Span, DocBlock>,
) -> Result<(Program, std::collections::HashMap<String, DocBlock>), String> {
    let mut blocks = Vec::new();
    let mut docs_by_name: std::collections::HashMap<String, DocBlock> =
        std::collections::HashMap::new();

    for cst_block in cst.blocks {
        let (block, block_docs) = lower_block(cst_block, &cst_docs)?;
        docs_by_name.extend(block_docs);
        blocks.push(block);
    }

    Ok((Program { blocks }, docs_by_name))
}

/// Lower a CST block to an AST block.
/// Returns the block and a map of documentation blocks keyed by declaration identifier name.
fn lower_block(
    cst: Spanned<CstBlock>,
    cst_docs: &std::collections::HashMap<Span, DocBlock>,
) -> Result<(Block, std::collections::HashMap<String, DocBlock>), String> {
    eprintln!(
        "[CST→AST] lower_block ptr={:p} stmt_count={}",
        &cst.node,
        cst.node.statements.len()
    );
    let mut statements = Vec::new();
    let mut docs_by_name: std::collections::HashMap<String, DocBlock> =
        std::collections::HashMap::new();

    for cst_stmt in cst.node.statements {
        let (stmt, stmt_doc) = lower_statement(cst_stmt, cst_docs)?;
        if let Some((name, doc)) = stmt_doc {
            docs_by_name.insert(name, doc);
        }
        statements.push(stmt);
    }

    Ok((Block { statements }, docs_by_name))
}

/// Lower a CST statement to an AST statement.
/// Returns the statement and optionally a doc block keyed by identifier name.
fn lower_statement(
    cst: Spanned<CstStatement>,
    cst_docs: &std::collections::HashMap<Span, DocBlock>,
) -> Result<(Statement, Option<(String, DocBlock)>), String> {
    match cst.node {
        CstStatement::Mod { identifier } => {
            let doc = cst_docs.get(&identifier.span).cloned();
            Ok((
                Statement::Mod {
                    identifier: make_ast_ident(&identifier),
                },
                doc.map(|d| (identifier.node, d)),
            ))
        }
        CstStatement::Let {
            pub_keyword,
            identifier,
            type_annotation,
            expression,
            ..
        } => {
            let pub_visibility = pub_keyword.is_some();
            let doc = cst_docs.get(&identifier.span).cloned();

            Ok((
                Statement::Let {
                    identifier: make_ast_ident(&identifier),
                    type_annotation: type_annotation.map(|t| t.node),
                    expression: lower_expr(expression, cst_docs)?,
                    pub_visibility,
                },
                doc.map(|d| (identifier.node.clone(), d)),
            ))
        }
        CstStatement::Const {
            pub_keyword,
            identifier,
            expression,
            ..
        } => {
            let pub_visibility = pub_keyword.is_some();
            let doc = cst_docs.get(&identifier.span).cloned();

            Ok((
                Statement::Const {
                    identifier: make_ast_ident(&identifier),
                    expression: lower_expr(expression, cst_docs)?,
                    pub_visibility,
                },
                doc.map(|d| (identifier.node.clone(), d)),
            ))
        }
        CstStatement::Assign {
            identifier,
            expression,
            ..
        } => Ok((
            Statement::Assign {
                identifier: make_ast_ident(&identifier),
                expression: lower_expr(expression, cst_docs)?,
            },
            None,
        )),
        CstStatement::AssignIncrement {
            identifier,
            expression,
            ..
        } => Ok((
            Statement::AssignIncrement {
                identifier: make_ast_ident(&identifier),
                expression: lower_expr(expression, cst_docs)?,
            },
            None,
        )),
        CstStatement::AssignDecrement {
            identifier,
            expression,
            ..
        } => Ok((
            Statement::AssignDecrement {
                identifier: make_ast_ident(&identifier),
                expression: lower_expr(expression, cst_docs)?,
            },
            None,
        )),
        CstStatement::If {
            arms, else_block, ..
        } => {
            let mut ast_arms = Vec::new();
            for (expr, block) in arms {
                let (b, _) = lower_block(block, cst_docs)?;
                ast_arms.push((lower_expr(expr, cst_docs)?, b));
            }

            Ok((
                Statement::If {
                    arms: ast_arms,
                    else_block: else_block
                        .map(|b| lower_block(b, cst_docs).map(|(block, _)| block))
                        .transpose()?,
                },
                None,
            ))
        }
        CstStatement::Match {
            expression, cases, ..
        } => {
            let mut ast_cases = Vec::new();
            for (pattern, block) in cases {
                let (b, _) = lower_block(block, cst_docs)?;
                ast_cases.push((pattern.map(|p| lower_expr(p, cst_docs)).transpose()?, b));
            }

            Ok((
                Statement::Match {
                    expression: lower_expr(expression, cst_docs)?,
                    cases: ast_cases,
                },
                None,
            ))
        }
        CstStatement::FunctionDeclaration {
            pub_keyword,
            identifier,
            arguments,
            return_type_arrow,
            body,
            ..
        } => {
            let pub_visibility = pub_keyword.is_some();
            let doc = cst_docs.get(&identifier.span).cloned();

            let mut ast_args = Vec::new();
            for arg in arguments {
                ast_args.push(Argument {
                    identifier: make_ast_ident(&arg.node.identifier),
                    kind: arg.node.type_annotation.node,
                });
            }

            let return_type = return_type_arrow.map(|rta| {
                format!(
                    "{} {}",
                    if rta.node.is_effectful { "~>" } else { "->" },
                    rta.node.type_annotation.node
                )
            });

            // body is already a Spanned<CstBlock>, so we can directly lower it
            eprintln!(
                "[CST→AST] lowering function body: spanned_ptr={:p}, block_node_ptr={:p}, id={:?}, stmt_count={}",
                &body,
                &body.node,
                body.id,
                body.node.statements.len()
            );
            let (body_block, _) = lower_block(body, cst_docs)?;

            Ok((
                Statement::FunctionDeclaration {
                    identifier: make_ast_ident(&identifier),
                    arguments: ast_args,
                    return_type,
                    body: body_block,
                    pub_visibility,
                    cst_id: cst.id, // Phase 3: FunctionDeclaration carries CstId
                },
                doc.map(|d| (identifier.node.clone(), d)),
            ))
        }
        CstStatement::Return { expression, .. } => Ok((
            Statement::Return {
                expression: lower_expr(expression, cst_docs)?,
            },
            None,
        )),
        CstStatement::Loop {
            init_vars, body, ..
        } => {
            let mut ast_init_vars = Vec::new();
            for (var, _, expr) in init_vars {
                ast_init_vars.push((make_ast_ident(&var), lower_expr(expr, cst_docs)?));
            }

            let (body_block, _) = lower_block(body, cst_docs)?;

            Ok((
                Statement::Loop {
                    init_vars: ast_init_vars,
                    body: body_block,
                },
                None,
            ))
        }
        CstStatement::While {
            condition, body, ..
        } => {
            let (body_block, _) = lower_block(body, cst_docs)?;

            Ok((
                Statement::While {
                    condition: lower_expr(condition, cst_docs)?,
                    body: body_block,
                },
                None,
            ))
        }
        CstStatement::For {
            var_name,
            start,
            end,
            body,
            ..
        } => {
            let (body_block, _) = lower_block(body, cst_docs)?;

            Ok((
                Statement::For {
                    var_name: make_ast_ident(&var_name),
                    start: lower_expr(start, cst_docs)?,
                    end: lower_expr(end, cst_docs)?,
                    body: body_block,
                },
                None,
            ))
        }
        CstStatement::Break { expression, .. } => Ok((
            Statement::Break {
                expression: expression.map(|e| lower_expr(e, cst_docs)).transpose()?,
            },
            None,
        )),
        CstStatement::Continue { .. } => Ok((Statement::Continue, None)),
        CstStatement::Use { selector, path, .. } => {
            let ast_selector = match selector.node {
                CstImportSelector::Single(name) => ImportSelector::Single(make_ast_ident(&name)),
                CstImportSelector::Multiple(names) => {
                    ImportSelector::Multiple(names.into_iter().map(|n| make_ast_ident(&n)).collect())
                }
                CstImportSelector::Wildcard(_) => ImportSelector::Wildcard,
            };

            Ok((
                Statement::Use {
                    path: path.into_iter().map(|p| make_ast_ident(&p)).collect(),
                    selector: ast_selector,
                },
                None,
            ))
        }
        CstStatement::Struct {
            pub_keyword,
            name,
            fields,
            ..
        } => {
            let pub_visibility = pub_keyword.is_some();
            let doc = cst_docs.get(&name.span).cloned();

            let mut ast_fields = Vec::new();
            for field in fields {
                ast_fields.push((make_ast_ident(&field.node.name), field.node.type_annotation.node));
            }

            Ok((
                Statement::Struct {
                    name: make_ast_ident(&name),
                    fields: ast_fields,
                    pub_visibility,
                },
                doc.map(|d| (name.node.clone(), d)),
            ))
        }
        CstStatement::Expression(expr) => {
            Ok((Statement::Expression(lower_expr(expr, cst_docs)?), None))
        }
    }
}

/// Lower a CST expression to an AST expression.
fn lower_expr(
    cst: Spanned<CstExpr>,
    cst_docs: &std::collections::HashMap<Span, DocBlock>,
) -> Result<Expression, String> {
    match cst.node {
        CstExpr::Literal(lit) => Ok(Expression::Literal(lower_literal(lit.node)?)),
        CstExpr::Identifier(id) => Ok(Expression::Identifier(make_ast_ident(&id))),
        CstExpr::FunctionCall {
            callee, arguments, ..
        } => {
            // Check if there are any holes - if so, create PartialCall
            let has_holes = arguments
                .iter()
                .any(|a| matches!(a.node, CstCallArgument::Hole(_)));
            if has_holes {
                let mut call_args = Vec::new();
                for arg in arguments {
                    match arg.node {
                        CstCallArgument::Expr(expr) => {
                            call_args.push(CallArgument::Expr(lower_expr(expr, cst_docs)?));
                        }
                        CstCallArgument::Hole(_) => {
                            call_args.push(CallArgument::Hole);
                        }
                    }
                }
                Ok(Expression::PartialCall {
                    func: Box::new(lower_expr(*callee, cst_docs)?),
                    args: call_args,
                })
            } else {
                let mut ast_args = Vec::new();
                for arg in arguments {
                    match arg.node {
                        CstCallArgument::Expr(expr) => {
                            ast_args.push(lower_expr(expr, cst_docs)?);
                        }
                        CstCallArgument::Hole(_) => {
                            // Shouldn't happen if has_holes was false, but handle gracefully
                            return Err(
                                "Unexpected hole in function call without partial application"
                                    .to_string(),
                            );
                        }
                    }
                }
                Ok(Expression::FunctionCall {
                    callee: Box::new(lower_expr(*callee, cst_docs)?),
                    arguments: ast_args,
                    cst_id: cst.id, // Phase 3: FunctionCall carries CstId
                })
            }
        }
        CstExpr::PartialCall { func, args, .. } => {
            let mut call_args = Vec::new();
            for arg in args {
                match arg.node {
                    CstCallArgument::Expr(expr) => {
                        call_args.push(CallArgument::Expr(lower_expr(expr, cst_docs)?));
                    }
                    CstCallArgument::Hole(_) => {
                        call_args.push(CallArgument::Hole);
                    }
                }
            }
            Ok(Expression::PartialCall {
                func: Box::new(lower_expr(*func, cst_docs)?),
                args: call_args,
            })
        }
        CstExpr::Prefix { op, rhs } => Ok(Expression::Prefix {
            op: lower_unary_op(op.node),
            rhs: Box::new(lower_expr(*rhs, cst_docs)?),
        }),
        CstExpr::Postfix { lhs, op } => {
            // For invoke (!), arguments are handled as a separate expression type in AST
            // The CST Postfix with Invoke op doesn't include arguments - they're part of the invoke syntax
            Ok(Expression::Postfix {
                lhs: Box::new(lower_expr(*lhs, cst_docs)?),
                op: lower_postfix_op(op.node),
                args: None, // Invoke arguments are not stored in CST Postfix node
            })
        }
        CstExpr::Infix { lhs, op, rhs } => Ok(Expression::Infix {
            lhs: Box::new(lower_expr(*lhs, cst_docs)?),
            op: lower_binary_op(op.node),
            rhs: Box::new(lower_expr(*rhs, cst_docs)?),
        }),
        CstExpr::Group { inner, .. } => {
            Ok(Expression::Group(Box::new(lower_expr(*inner, cst_docs)?)))
        }
        CstExpr::Loop {
            init_vars, body, ..
        } => {
            let mut ast_init_vars = Vec::new();
            for (var, _, expr) in init_vars {
                ast_init_vars.push((make_ast_ident(&var), lower_expr(expr, cst_docs)?));
            }
            let (body_block, _) = lower_block(body, cst_docs)?;
            Ok(Expression::Loop {
                init_vars: ast_init_vars,
                body: body_block,
            })
        }
        CstExpr::Compose { lhs, op, rhs } => Ok(Expression::Compose {
            lhs: Box::new(lower_expr(*lhs, cst_docs)?),
            rhs: Box::new(lower_expr(*rhs, cst_docs)?),
            reverse: matches!(op.node, CstComposeOp::Reverse),
        }),
        CstExpr::MemberAccess {
            object, members, ..
        } => {
            // For multi-level member access (e.g., "utils.add"), use the last member
            // Phase 3: member carries CstId for identity tracking
            let last_member = members.last().unwrap();
            Ok(Expression::MemberAccess {
                object: Box::new(lower_expr(*object, cst_docs)?),
                member: make_ast_ident(last_member),
                cst_id: cst.id, // Phase 3: MemberAccess carries CstId
            })
        }
        CstExpr::StructInit {
            struct_name,
            fields,
            ..
        } => {
            let mut ast_fields = Vec::new();
            for field in fields {
                ast_fields.push((
                    make_ast_ident(&field.node.name),
                    lower_expr(field.node.value, cst_docs)?,
                ));
            }
            Ok(Expression::StructInit {
                struct_name: make_ast_ident(&struct_name),
                fields: ast_fields,
                cst_id: cst.id, // Phase 3: StructInit carries CstId
            })
        }
        CstExpr::FieldAccess { object, field, .. } => {
            Ok(Expression::FieldAccess {
                object: Box::new(lower_expr(*object, cst_docs)?),
                field: make_ast_ident(&field),
                cst_id: cst.id, // Phase 3: FieldAccess carries CstId
            })
        }
        CstExpr::Array { elements, .. } => {
            let mut ast_elements = Vec::new();
            for elem in elements {
                ast_elements.push(lower_expr(elem, cst_docs)?);
            }
            Ok(Expression::Array(ast_elements))
        }
        CstExpr::ArrayIndex { array, indices, .. } => {
            let mut ast_indices = Vec::new();
            for idx in indices {
                ast_indices.push(lower_index_spec(idx.node, cst_docs)?);
            }
            Ok(Expression::ArrayIndex {
                array: Box::new(lower_expr(*array, cst_docs)?),
                indices: ast_indices,
            })
        }
        CstExpr::Closure {
            arguments,
            return_type_arrow,
            body,
            ..
        } => {
            let mut ast_args = Vec::new();
            for arg in arguments {
                ast_args.push(Argument {
                    identifier: make_ast_ident(&arg.node.identifier),
                    kind: arg.node.type_annotation.map(|t| t.node).unwrap_or_default(),
                });
            }

            let return_type = return_type_arrow.map(|rta| rta.node.type_annotation.node);

            let ast_body = match body {
                CstClosureBody::Expression(expr) => {
                    ClosureBody::Expression(Box::new(lower_expr(*expr, cst_docs)?))
                }
                CstClosureBody::Block(block) => {
                    eprintln!(
                        "[CST→AST] lowering closure body ptr={:p} stmt_count={}",
                        &block.node,
                        block.node.statements.len()
                    );
                    let (block_ast, _) = lower_block(block, cst_docs)?;
                    ClosureBody::Block(block_ast)
                }
            };

            Ok(Expression::Closure {
                arguments: ast_args,
                return_type,
                body: ast_body,
            })
        }
    }
}

/// Lower a CST literal to an AST literal.
fn lower_literal(cst: CstLiteral) -> Result<Literal, String> {
    match cst {
        CstLiteral::String(s) => Ok(Literal::String(s)),
        CstLiteral::Number(n) => Ok(Literal::Number(n)),
        CstLiteral::Boolean(b) => Ok(Literal::Boolean(b)),
    }
}

/// Lower a CST unary operator to an AST unary operator.
fn lower_unary_op(cst: CstUnaryOp) -> UnaryOp {
    match cst {
        CstUnaryOp::Neg => UnaryOp::Neg,
        CstUnaryOp::Increment => UnaryOp::Increment,
        CstUnaryOp::Decrement => UnaryOp::Decrement,
        CstUnaryOp::Not => UnaryOp::Not,
    }
}

/// Lower a CST postfix operator to an AST postfix operator.
fn lower_postfix_op(cst: CstPostfixOp) -> PostfixOp {
    match cst {
        CstPostfixOp::Invoke => PostfixOp::Invoke,
    }
}

/// Lower a CST binary operator to an AST binary operator.
fn lower_binary_op(cst: CstBinaryOp) -> BinaryOp {
    match cst {
        CstBinaryOp::Add => BinaryOp::Add,
        CstBinaryOp::Sub => BinaryOp::Sub,
        CstBinaryOp::Mul => BinaryOp::Mul,
        CstBinaryOp::Div => BinaryOp::Div,
        CstBinaryOp::Mod => BinaryOp::Mod,
        CstBinaryOp::Pow => BinaryOp::Pow,
        CstBinaryOp::Eq => BinaryOp::Eq,
        CstBinaryOp::Ne => BinaryOp::Ne,
        CstBinaryOp::Gt => BinaryOp::Gt,
        CstBinaryOp::Lt => BinaryOp::Lt,
        CstBinaryOp::Ge => BinaryOp::Ge,
        CstBinaryOp::Le => BinaryOp::Le,
        CstBinaryOp::And => BinaryOp::And,
        CstBinaryOp::Or => BinaryOp::Or,
    }
}

/// Lower a CST index spec to an AST index spec.
fn lower_index_spec(
    cst: CstIndexSpec,
    cst_docs: &std::collections::HashMap<Span, DocBlock>,
) -> Result<IndexSpec, String> {
    match cst {
        CstIndexSpec::Single(expr) => Ok(IndexSpec::Single(lower_expr(expr, cst_docs)?)),
        CstIndexSpec::Range {
            start, end, step, ..
        } => Ok(IndexSpec::Range {
            start: start.map(|e| lower_expr(e, cst_docs)).transpose()?,
            end: end.map(|e| lower_expr(e, cst_docs)).transpose()?,
            step: step.map(|(_, e)| lower_expr(e, cst_docs)).transpose()?,
        }),
        CstIndexSpec::InclusiveRange { start, end, .. } => Ok(IndexSpec::InclusiveRange {
            start: start.map(|e| lower_expr(e, cst_docs)).transpose()?,
            end: end.map(|e| lower_expr(e, cst_docs)).transpose()?,
        }),
    }
}
