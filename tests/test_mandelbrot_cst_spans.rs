use cantaloop::core::cst::{parse_cst_program, CstExpr, CstStatement};

fn slice<'a>(src: &'a str, start: u32, end: u32) -> &'a str {
    &src[start as usize..end as usize]
}

#[test]
fn mandelbrot_struct_field_type_spans_are_exact() {
    let src = std::fs::read_to_string("tests/fixtures/mandelbrot/main.cl").expect("read mandelbrot fixture");
    let (cst, _) = parse_cst_program(&src).expect("parse CST");

    let mut found = false;
    for block in &cst.blocks {
        for stmt in &block.node.statements {
            if let CstStatement::Struct { name, fields, .. } = &stmt.node {
                if name.node == "State" {
                    found = true;
                    for f in fields {
                        let field_name = slice(&src, f.node.name.span.start, f.node.name.span.end);
                        assert_eq!(field_name, f.node.name.node.as_str());

                        let ty = slice(&src, f.node.type_annotation.span.start, f.node.type_annotation.span.end);
                        assert_eq!(
                            ty,
                            f.node.type_annotation.node.as_str(),
                            "type span should match exactly for field {}",
                            field_name
                        );
                    }
                }
            }
        }
    }

    assert!(found, "expected to find `struct State` in mandelbrot CST");
}

#[test]
fn mandelbrot_closure_body_member_access_spans_are_exact() {
    let src = std::fs::read_to_string("tests/fixtures/mandelbrot/main.cl").expect("read mandelbrot fixture");
    let (cst, _) = parse_cst_program(&src).expect("parse CST");

    fn walk_expr(src: &str, expr: &cantaloop::core::cst::Spanned<CstExpr>) {
        match &expr.node {
            CstExpr::Identifier(id) => {
                assert_eq!(slice(src, id.span.start, id.span.end), id.node.as_str());
            }
            CstExpr::Literal(lit) => {
                // Literal span must be absolute and must slice valid source.
                let s = slice(src, lit.span.start, lit.span.end);
                match &lit.node {
                    cantaloop::core::cst::CstLiteral::String(_) => {
                        assert!(s.starts_with('"') && s.ends_with('"'), "string literal span should include quotes; got {s:?}");
                    }
                    _ => {}
                }
            }
            CstExpr::MemberAccess { object, members, .. } => {
                walk_expr(src, object);
                for m in members {
                    assert_eq!(slice(src, m.span.start, m.span.end), m.node.as_str());
                }
            }
            CstExpr::FieldAccess { object, field, .. } => {
                walk_expr(src, object);
                assert_eq!(slice(src, field.span.start, field.span.end), field.node.as_str());
            }
            CstExpr::FunctionCall { callee, arguments, .. } => {
                walk_expr(src, callee);
                for a in arguments {
                    if let cantaloop::core::cst::CstCallArgument::Expr(e) = &a.node {
                        walk_expr(src, e);
                    }
                }
            }
            CstExpr::PartialCall { func, args, .. } => {
                walk_expr(src, func);
                for a in args {
                    if let cantaloop::core::cst::CstCallArgument::Expr(e) = &a.node {
                        walk_expr(src, e);
                    }
                }
            }
            CstExpr::Compose { lhs, rhs, .. } | CstExpr::Infix { lhs, rhs, .. } => {
                walk_expr(src, lhs);
                walk_expr(src, rhs);
            }
            CstExpr::Prefix { rhs, .. } => walk_expr(src, rhs),
            CstExpr::Postfix { lhs, .. } => walk_expr(src, lhs),
            CstExpr::Group { inner, .. } => walk_expr(src, inner),
            CstExpr::Array { elements, .. } => {
                for e in elements {
                    walk_expr(src, e);
                }
            }
            CstExpr::ArrayIndex { array, indices, .. } => {
                walk_expr(src, array);
                for idx in indices {
                    match &idx.node {
                        cantaloop::core::cst::CstIndexSpec::Single(e) => walk_expr(src, e),
                        cantaloop::core::cst::CstIndexSpec::Range { start, end, .. }
                        | cantaloop::core::cst::CstIndexSpec::InclusiveRange { start, end, .. } => {
                            if let Some(s) = start {
                                walk_expr(src, s);
                            }
                            if let Some(e) = end {
                                walk_expr(src, e);
                            }
                        }
                    }
                }
            }
            CstExpr::Loop { init_vars, .. } => {
                for (v, _, e) in init_vars {
                    assert_eq!(slice(src, v.span.start, v.span.end), v.node.as_str());
                    walk_expr(src, e);
                }
            }
            CstExpr::StructInit { struct_name, fields, .. } => {
                let got = slice(src, struct_name.span.start, struct_name.span.end);
                if got != struct_name.node.as_str() {
                    let s = struct_name.span.start as usize;
                    let e = struct_name.span.end as usize;
                    let ctx_start = s.saturating_sub(20);
                    let ctx_end = (e + 20).min(src.len());
                    panic!(
                        "StructInit struct_name span mismatch: expected {:?}, got {:?}, span={:?}, ctx={:?}",
                        struct_name.node,
                        got,
                        struct_name.span,
                        &src[ctx_start..ctx_end]
                    );
                }
                for f in fields {
                    assert_eq!(slice(src, f.node.name.span.start, f.node.name.span.end), f.node.name.node.as_str());
                    walk_expr(src, &f.node.value);
                }
            }
            CstExpr::Closure { arguments, return_type_arrow, body, .. } => {
                for a in arguments {
                    if !a.node.is_placeholder {
                        assert_eq!(slice(src, a.node.identifier.span.start, a.node.identifier.span.end), a.node.identifier.node.as_str());
                    }
                    if let Some(ty) = &a.node.type_annotation {
                        assert_eq!(slice(src, ty.span.start, ty.span.end), ty.node.as_str());
                    }
                }
                if let Some(rta) = return_type_arrow {
                    assert!(!slice(src, rta.node.type_annotation.span.start, rta.node.type_annotation.span.end).is_empty());
                }
                match body {
                    cantaloop::core::cst::CstClosureBody::Expression(e) => walk_expr(src, e),
                    cantaloop::core::cst::CstClosureBody::Block(b) => {
                        for s in &b.node.statements {
                            match &s.node {
                                CstStatement::Let { identifier, type_annotation, expression, .. } => {
                                    assert_eq!(slice(src, identifier.span.start, identifier.span.end), identifier.node.as_str());
                                    if let Some(ty) = type_annotation {
                                        assert!(!slice(src, ty.span.start, ty.span.end).is_empty());
                                    }
                                    walk_expr(src, expression);
                                }
                                CstStatement::Expression(e) => walk_expr(src, e),
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }

    for block in &cst.blocks {
        for stmt in &block.node.statements {
            match &stmt.node {
                CstStatement::Let { identifier, type_annotation, expression, .. } => {
                    assert_eq!(slice(&src, identifier.span.start, identifier.span.end), identifier.node.as_str());
                    if let Some(ty) = type_annotation {
                        assert!(!slice(&src, ty.span.start, ty.span.end).is_empty());
                    }
                    walk_expr(&src, expression);
                }
                CstStatement::Struct { name, fields, .. } => {
                    assert_eq!(slice(&src, name.span.start, name.span.end), name.node.as_str());
                    for f in fields {
                        assert_eq!(slice(&src, f.node.name.span.start, f.node.name.span.end), f.node.name.node.as_str());
                        assert_eq!(slice(&src, f.node.type_annotation.span.start, f.node.type_annotation.span.end), f.node.type_annotation.node.as_str());
                    }
                }
                CstStatement::Expression(e) => walk_expr(&src, e),
                _ => {}
            }
        }
    }
}

#[test]
fn mandelbrot_scale_line_has_three_distinct_min_identifier_spans() {
    use cantaloop::core::cst::{CstExpr, CstStatement};

    let src = std::fs::read_to_string("tests/fixtures/mandelbrot/main.cl").expect("read mandelbrot fixture");
    let (cst, _) = parse_cst_program(&src).expect("parse CST");

    // Collect all identifier spans that slice to "min".
    let mut mins: Vec<(u32, u32)> = Vec::new();

    fn walk_expr(src: &str, expr: &cantaloop::core::cst::Spanned<CstExpr>, mins: &mut Vec<(u32, u32)>) {
        match &expr.node {
            CstExpr::Identifier(id) => {
                if &src[id.span.start as usize..id.span.end as usize] == "min" {
                    mins.push((id.span.start, id.span.end));
                }
            }
            CstExpr::Infix { lhs, rhs, .. } | CstExpr::Compose { lhs, rhs, .. } => {
                walk_expr(src, lhs, mins);
                walk_expr(src, rhs, mins);
            }
            CstExpr::Prefix { rhs, .. } => walk_expr(src, rhs, mins),
            CstExpr::Postfix { lhs, .. } => walk_expr(src, lhs, mins),
            CstExpr::Group { inner, .. } => walk_expr(src, inner, mins),
            CstExpr::FunctionCall { callee, arguments, .. } => {
                walk_expr(src, callee, mins);
                for a in arguments {
                    if let cantaloop::core::cst::CstCallArgument::Expr(e) = &a.node {
                        walk_expr(src, e, mins);
                    }
                }
            }
            CstExpr::PartialCall { func, args, .. } => {
                walk_expr(src, func, mins);
                for a in args {
                    if let cantaloop::core::cst::CstCallArgument::Expr(e) = &a.node {
                        walk_expr(src, e, mins);
                    }
                }
            }
            CstExpr::ArrayIndex { array, indices, .. } => {
                walk_expr(src, array, mins);
                for idx in indices {
                    match &idx.node {
                        cantaloop::core::cst::CstIndexSpec::Single(e) => walk_expr(src, e, mins),
                        cantaloop::core::cst::CstIndexSpec::Range { start, end, .. }
                        | cantaloop::core::cst::CstIndexSpec::InclusiveRange { start, end, .. } => {
                            if let Some(s) = start {
                                walk_expr(src, s, mins);
                            }
                            if let Some(e) = end {
                                walk_expr(src, e, mins);
                            }
                        }
                    }
                }
            }
            CstExpr::Closure { arguments, body, .. } => {
                for a in arguments {
                    if !a.node.is_placeholder && &src[a.node.identifier.span.start as usize..a.node.identifier.span.end as usize] == "min" {
                        mins.push((a.node.identifier.span.start, a.node.identifier.span.end));
                    }
                }
                match body {
                    cantaloop::core::cst::CstClosureBody::Expression(e) => walk_expr(src, e, mins),
                    cantaloop::core::cst::CstClosureBody::Block(b) => {
                        for st in &b.node.statements {
                            match &st.node {
                                CstStatement::Let { expression, .. } => walk_expr(src, expression, mins),
                                CstStatement::Expression(e) => walk_expr(src, e, mins),
                                CstStatement::Return { expression, .. } => walk_expr(src, expression, mins),
                                _ => {}
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    for block in &cst.blocks {
        for stmt in &block.node.statements {
            if let CstStatement::Let { expression, .. } = &stmt.node {
                walk_expr(&src, expression, &mut mins);
            }
        }
    }

    mins.sort();
    mins.dedup();

    // Expect:
    // - param `min` in `min: num`
    // - inner `min` in `(max - min)`
    // - trailing `min` in `+ min`
    assert!(
        mins.len() >= 3,
        "expected at least 3 distinct `min` identifier spans, got {:?}",
        mins.iter()
            .map(|(s, e)| (&src[*s as usize..*e as usize], *s, *e))
            .collect::<Vec<_>>()
    );
}

#[test]
fn mandelbrot_to_char_gradient_index_expression_spans_are_exact() {
    use cantaloop::core::cst::{parse_cst_program, CstExpr, CstStatement};

    let src = std::fs::read_to_string("tests/fixtures/mandelbrot/main.cl").expect("read mandelbrot fixture");
    let (cst, _) = parse_cst_program(&src).expect("parse CST");

    // We want to ensure the ArrayIndex inner expression spans correctly cover:
    // - `gradient[` base identifier
    // - `math.floor` member access
    // - `string.len` member access
    let gradient_off = src.find("gradient[").expect("find gradient[");
    let floor_off = src.find("math.floor").expect("find math.floor");
    let len_off = src.find("string.len").expect("find string.len");

    fn slice<'a>(s: &'a str, start: u32, end: u32) -> &'a str {
        &s[start as usize..end as usize]
    }

    fn collect_exprs_at_offset<'a>(
        expr: &'a cantaloop::core::cst::Spanned<CstExpr>,
        offset: usize,
        out: &mut Vec<&'a cantaloop::core::cst::Spanned<CstExpr>>,
    ) {
        let sp = expr.span;
        if sp.start as usize <= offset && offset < sp.end as usize {
            out.push(expr);
            match &expr.node {
                CstExpr::Infix { lhs, rhs, .. } | CstExpr::Compose { lhs, rhs, .. } => {
                    collect_exprs_at_offset(lhs, offset, out);
                    collect_exprs_at_offset(rhs, offset, out);
                }
                CstExpr::Prefix { rhs, .. } => collect_exprs_at_offset(rhs, offset, out),
                CstExpr::Postfix { lhs, .. } => collect_exprs_at_offset(lhs, offset, out),
                CstExpr::Group { inner, .. } => collect_exprs_at_offset(inner, offset, out),
                CstExpr::FunctionCall { callee, arguments, .. } => {
                    collect_exprs_at_offset(callee, offset, out);
                    for a in arguments {
                        if let cantaloop::core::cst::CstCallArgument::Expr(e) = &a.node {
                            collect_exprs_at_offset(e, offset, out);
                        }
                    }
                }
                CstExpr::MemberAccess { object, .. } => collect_exprs_at_offset(object, offset, out),
                CstExpr::FieldAccess { object, .. } => collect_exprs_at_offset(object, offset, out),
                CstExpr::ArrayIndex { array, indices, .. } => {
                    collect_exprs_at_offset(array, offset, out);
                    for idx in indices {
                        match &idx.node {
                            cantaloop::core::cst::CstIndexSpec::Single(e) => collect_exprs_at_offset(e, offset, out),
                            cantaloop::core::cst::CstIndexSpec::Range { start, end, .. }
                            | cantaloop::core::cst::CstIndexSpec::InclusiveRange { start, end, .. } => {
                                if let Some(s) = start {
                                    collect_exprs_at_offset(s, offset, out);
                                }
                                if let Some(e) = end {
                                    collect_exprs_at_offset(e, offset, out);
                                }
                            }
                        }
                    }
                }
                CstExpr::Closure { body, .. } => match body {
                    cantaloop::core::cst::CstClosureBody::Expression(e) => collect_exprs_at_offset(e, offset, out),
                    cantaloop::core::cst::CstClosureBody::Block(b) => {
                        for st in &b.node.statements {
                            match &st.node {
                                CstStatement::Let { expression, .. } => collect_exprs_at_offset(expression, offset, out),
                                CstStatement::Const { expression, .. } => collect_exprs_at_offset(expression, offset, out),
                                CstStatement::Expression(e) => collect_exprs_at_offset(e, offset, out),
                                CstStatement::Return { expression, .. } => collect_exprs_at_offset(expression, offset, out),
                                CstStatement::If { arms, else_block, .. } => {
                                    for (cond, blk) in arms {
                                        collect_exprs_at_offset(cond, offset, out);
                                        for st2 in &blk.node.statements {
                                            if let CstStatement::Expression(e2) = &st2.node {
                                                collect_exprs_at_offset(e2, offset, out);
                                            }
                                            if let CstStatement::Return { expression, .. } = &st2.node {
                                                collect_exprs_at_offset(expression, offset, out);
                                            }
                                            if let CstStatement::Let { expression, .. } = &st2.node {
                                                collect_exprs_at_offset(expression, offset, out);
                                            }
                                        }
                                    }
                                    if let Some(eb) = else_block {
                                        for st2 in &eb.node.statements {
                                            match &st2.node {
                                                CstStatement::Let { expression, .. } => collect_exprs_at_offset(expression, offset, out),
                                                CstStatement::Expression(e2) => collect_exprs_at_offset(e2, offset, out),
                                                CstStatement::Return { expression, .. } => collect_exprs_at_offset(expression, offset, out),
                                                _ => {}
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                },
                _ => {}
            }
        }
    }

    // Find the smallest member-access expression that contains each needle.
    let mut at_grad = Vec::new();
    let mut at_floor = Vec::new();
    let mut at_len = Vec::new();

    for block in &cst.blocks {
        for stmt in &block.node.statements {
            match &stmt.node {
                CstStatement::Let { expression, .. } | CstStatement::Const { expression, .. } => {
                    collect_exprs_at_offset(expression, gradient_off, &mut at_grad);
                    collect_exprs_at_offset(expression, floor_off, &mut at_floor);
                    collect_exprs_at_offset(expression, len_off, &mut at_len);
                }
                CstStatement::Expression(e) => {
                    collect_exprs_at_offset(e, gradient_off, &mut at_grad);
                    collect_exprs_at_offset(e, floor_off, &mut at_floor);
                    collect_exprs_at_offset(e, len_off, &mut at_len);
                }
                _ => {}
            }
        }
    }

    let grad_expr = at_grad
        .iter()
        .filter(|e| matches!(e.node, CstExpr::ArrayIndex { .. }))
        .min_by_key(|e| e.span.end - e.span.start)
        .expect("expected ArrayIndex containing gradient[");
    assert!(slice(&src, grad_expr.span.start, grad_expr.span.end).contains("gradient["), "bad ArrayIndex span slice: {:?}", slice(&src, grad_expr.span.start, grad_expr.span.end));

    let floor_expr = at_floor
        .iter()
        .filter(|e| matches!(e.node, CstExpr::MemberAccess { .. }))
        .min_by_key(|e| e.span.end - e.span.start)
        .expect("expected MemberAccess containing math.floor");
    assert!(slice(&src, floor_expr.span.start, floor_expr.span.end).contains("math.floor"), "bad math.floor span slice: {:?}", slice(&src, floor_expr.span.start, floor_expr.span.end));

    let len_expr = at_len
        .iter()
        .filter(|e| matches!(e.node, CstExpr::MemberAccess { .. }))
        .min_by_key(|e| e.span.end - e.span.start)
        .expect("expected MemberAccess containing string.len");
    assert!(slice(&src, len_expr.span.start, len_expr.span.end).contains("string.len"), "bad string.len span slice: {:?}", slice(&src, len_expr.span.start, len_expr.span.end));
}

#[test]
fn mandelbrot_to_char_if_condition_spans_are_exact() {
    use cantaloop::core::cst::{parse_cst_program, CstExpr, CstStatement};

    let src = std::fs::read_to_string("tests/fixtures/mandelbrot/main.cl").expect("read mandelbrot fixture");
    let (cst, _) = parse_cst_program(&src).expect("parse CST");

    // Inside `to_char`, we expect the condition to slice exactly as `v == max_iter`.
    let if_off = src.find("if v == max_iter").expect("find if v == max_iter");
    let v_off = if_off + "if ".len();
    let max_iter_off = src[if_off..]
        .find("max_iter")
        .map(|i| if_off + i)
        .expect("find max_iter in if condition");

    fn slice<'a>(s: &'a str, start: u32, end: u32) -> &'a str {
        &s[start as usize..end as usize]
    }

    fn walk_expr_find_ident<'a>(
        src: &'a str,
        expr: &'a cantaloop::core::cst::Spanned<CstExpr>,
        want_off: usize,
    ) -> Option<&'a cantaloop::core::cst::Spanned<String>> {
        let sp = expr.span;
        if !(sp.start as usize <= want_off && want_off < sp.end as usize) {
            return None;
        }
        match &expr.node {
            CstExpr::Identifier(id) => Some(id),
            CstExpr::Infix { lhs, rhs, .. } | CstExpr::Compose { lhs, rhs, .. } => {
                walk_expr_find_ident(src, lhs, want_off).or_else(|| walk_expr_find_ident(src, rhs, want_off))
            }
            CstExpr::Prefix { rhs, .. } => walk_expr_find_ident(src, rhs, want_off),
            CstExpr::Postfix { lhs, .. } => walk_expr_find_ident(src, lhs, want_off),
            CstExpr::Group { inner, .. } => walk_expr_find_ident(src, inner, want_off),
            _ => None,
        }
    }

    // Find the If statement in the CST and check its condition identifier spans.
    let mut found = false;
    for block in &cst.blocks {
        for stmt in &block.node.statements {
            if let CstStatement::Let { identifier, expression, .. } = &stmt.node {
                if identifier.node == "to_char" {
                    if let CstExpr::Closure { body, .. } = &expression.node {
                        if let cantaloop::core::cst::CstClosureBody::Block(b) = body {
                            for st in &b.node.statements {
                                if let CstStatement::If { arms, .. } = &st.node {
                                    let (cond, _) = &arms[0];
                                    let v_ident = walk_expr_find_ident(&src, cond, v_off).expect("find v ident in condition");
                                    assert_eq!(slice(&src, v_ident.span.start, v_ident.span.end), "v");

                                    let m_ident = walk_expr_find_ident(&src, cond, max_iter_off).expect("find max_iter ident in condition");
                                    assert_eq!(slice(&src, m_ident.span.start, m_ident.span.end), "max_iter");
                                    found = true;
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    assert!(found, "expected to find `if v == max_iter` condition inside `to_char`");
}

#[test]
fn mandelbrot_fold_state_struct_init_span_is_exact() {
    use cantaloop::core::cst::{parse_cst_program, CstExpr, CstStatement};

    let src = std::fs::read_to_string("tests/fixtures/mandelbrot/main.cl").expect("read mandelbrot fixture");
    let (cst, _) = parse_cst_program(&src).expect("parse CST");

    let off = src.find("State { zx: 0").expect("find fold initializer State {");

    fn slice<'a>(s: &'a str, start: u32, end: u32) -> &'a str {
        &s[start as usize..end as usize]
    }

    fn walk_expr<'a>(src: &'a str, expr: &'a cantaloop::core::cst::Spanned<CstExpr>, off: usize) -> bool {
        let sp = expr.span;
        if !(sp.start as usize <= off && off < sp.end as usize) {
            return false;
        }
        match &expr.node {
            CstExpr::StructInit { struct_name, .. } => {
                return slice(src, struct_name.span.start, struct_name.span.end) == "State";
            }
            CstExpr::FunctionCall { callee, arguments, .. } => {
                if walk_expr(src, callee, off) {
                    return true;
                }
                for a in arguments {
                    if let cantaloop::core::cst::CstCallArgument::Expr(e) = &a.node {
                        if walk_expr(src, e, off) {
                            return true;
                        }
                    }
                }
            }
            CstExpr::Infix { lhs, rhs, .. } | CstExpr::Compose { lhs, rhs, .. } => {
                if walk_expr(src, lhs, off) {
                    return true;
                }
                if walk_expr(src, rhs, off) {
                    return true;
                }
            }
            CstExpr::Prefix { rhs, .. } => return walk_expr(src, rhs, off),
            CstExpr::Postfix { lhs, .. } => return walk_expr(src, lhs, off),
            CstExpr::Group { inner, .. } => return walk_expr(src, inner, off),
            CstExpr::Closure { body, .. } => {
                if let cantaloop::core::cst::CstClosureBody::Block(b) = body {
                    for st in &b.node.statements {
                        match &st.node {
                            CstStatement::Let { expression, .. }
                            | CstStatement::Const { expression, .. }
                            | CstStatement::Assign { expression, .. } => {
                                if walk_expr(src, expression, off) {
                                    return true;
                                }
                            }
                            CstStatement::Expression(e) => {
                                if walk_expr(src, e, off) {
                                    return true;
                                }
                            }
                            CstStatement::Return { expression, .. } => {
                                if walk_expr(src, expression, off) {
                                    return true;
                                }
                            }
                            CstStatement::If { arms, else_block, .. } => {
                                for (cond, blk) in arms {
                                    if walk_expr(src, cond, off) {
                                        return true;
                                    }
                                    for s2 in &blk.node.statements {
                                        if let CstStatement::Expression(e2) = &s2.node {
                                            if walk_expr(src, e2, off) {
                                                return true;
                                            }
                                        }
                                    }
                                }
                                if let Some(eb) = else_block {
                                    for s2 in &eb.node.statements {
                                        if let CstStatement::Expression(e2) = &s2.node {
                                            if walk_expr(src, e2, off) {
                                                return true;
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
        false
    }

    let mut found = false;
    for block in &cst.blocks {
        for stmt in &block.node.statements {
            match &stmt.node {
                CstStatement::Let { expression, .. } => {
                    if walk_expr(&src, expression, off) {
                        found = true;
                        break;
                    }
                }
                _ => {}
            }
        }
    }

    assert!(found, "expected to find StructInit `State {{ zx: 0, ... }}` in fold args");
}

#[test]
fn mandelbrot_final_iter_spans_are_exact() {
    use cantaloop::core::cst::{parse_cst_program, CstExpr, CstStatement};

    let src = std::fs::read_to_string("tests/fixtures/mandelbrot/main.cl").expect("read mandelbrot fixture");
    let (cst, _) = parse_cst_program(&src).expect("parse CST");

    let off = src.find("final.iter").expect("find final.iter");

    fn slice<'a>(s: &'a str, start: u32, end: u32) -> &'a str {
        &s[start as usize..end as usize]
    }

    fn walk_expr_all<'a>(
        src: &'a str,
        expr: &'a cantaloop::core::cst::Spanned<CstExpr>,
        off: usize,
        out: &mut Vec<(&'a str, &'a str)>,
    ) {
        match &expr.node {
            CstExpr::FieldAccess { object, field, .. } => {
                if let CstExpr::Identifier(obj) = &object.node {
                    if expr.span.start as usize <= off && off < expr.span.end as usize {
                        out.push((
                            slice(src, obj.span.start, obj.span.end),
                            slice(src, field.span.start, field.span.end),
                        ));
                    }
                }
                walk_expr_all(src, object, off, out);
            }
            CstExpr::MemberAccess { object, members, .. } => {
                if let CstExpr::Identifier(obj) = &object.node {
                    if let Some(last) = members.last() {
                        if expr.span.start as usize <= off && off < expr.span.end as usize {
                            out.push((
                                slice(src, obj.span.start, obj.span.end),
                                slice(src, last.span.start, last.span.end),
                            ));
                        }
                    }
                }
                walk_expr_all(src, object, off, out);
            }
            CstExpr::Infix { lhs, rhs, .. } | CstExpr::Compose { lhs, rhs, .. } => {
                walk_expr_all(src, lhs, off, out);
                walk_expr_all(src, rhs, off, out);
            }
            CstExpr::Prefix { rhs, .. } => walk_expr_all(src, rhs, off, out),
            CstExpr::Postfix { lhs, .. } => walk_expr_all(src, lhs, off, out),
            CstExpr::Group { inner, .. } => walk_expr_all(src, inner, off, out),
            CstExpr::FunctionCall { callee, arguments, .. } => {
                walk_expr_all(src, callee, off, out);
                for a in arguments {
                    if let cantaloop::core::cst::CstCallArgument::Expr(e) = &a.node {
                        walk_expr_all(src, e, off, out);
                    }
                }
            }
            CstExpr::PartialCall { func, args, .. } => {
                walk_expr_all(src, func, off, out);
                for a in args {
                    if let cantaloop::core::cst::CstCallArgument::Expr(e) = &a.node {
                        walk_expr_all(src, e, off, out);
                    }
                }
            }
            CstExpr::Closure { body, .. } => match body {
                cantaloop::core::cst::CstClosureBody::Expression(e) => walk_expr_all(src, e, off, out),
                cantaloop::core::cst::CstClosureBody::Block(b) => {
                    for st in &b.node.statements {
                        walk_stmt_all(src, st, off, out);
                    }
                }
            },
            _ => {}
        }
    }

    fn walk_stmt_all<'a>(
        src: &'a str,
        stmt: &'a cantaloop::core::cst::Spanned<CstStatement>,
        off: usize,
        out: &mut Vec<(&'a str, &'a str)>,
    ) {
        match &stmt.node {
            CstStatement::Let { expression, .. }
            | CstStatement::Const { expression, .. }
            | CstStatement::Assign { expression, .. }
            | CstStatement::AssignIncrement { expression, .. }
            | CstStatement::AssignDecrement { expression, .. } => walk_expr_all(src, expression, off, out),
            CstStatement::Expression(e) => walk_expr_all(src, e, off, out),
            CstStatement::Return { expression, .. } => walk_expr_all(src, expression, off, out),
            CstStatement::If { arms, else_block, .. } => {
                for (cond, blk) in arms {
                    walk_expr_all(src, cond, off, out);
                    for st in &blk.node.statements {
                        walk_stmt_all(src, st, off, out);
                    }
                }
                if let Some(eb) = else_block {
                    for st in &eb.node.statements {
                        walk_stmt_all(src, st, off, out);
                    }
                }
            }
            _ => {}
        }
    }

    let mut hits: Vec<(&str, &str)> = Vec::new();
    for block in &cst.blocks {
        for stmt in &block.node.statements {
            walk_stmt_all(&src, stmt, off, &mut hits);
        }
    }

    let (obj, field) = hits.into_iter().find(|(o, f)| *o == "final" && *f == "iter")
        .expect("expected to find final.iter access in CST");
    assert_eq!(obj, "final");
    assert_eq!(field, "iter");
}

