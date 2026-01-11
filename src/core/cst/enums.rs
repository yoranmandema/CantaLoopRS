use serde::Serialize;

use super::{Span, Spanned};

/// Documentation block for a declaration.
/// 
/// Contains the raw text from consecutive `///` lines.
/// Also parses structured tags like @param, @returns, etc.
#[derive(Debug, Clone, Serialize)]
pub struct DocBlock {
    pub span: Span,
    /// Raw text content (all lines joined with newlines)
    pub text: String,
    /// Parsed documentation tags
    pub tags: DocTags,
}

/// Parsed documentation tags from doc comments.
#[derive(Debug, Clone, Default, Serialize)]
pub struct DocTags {
    /// Parameter documentation: parameter name -> description
    pub params: std::collections::HashMap<String, String>,
    /// Return value documentation
    pub returns: Option<String>,
    /// Effects documentation (e.g., @effects, !effects)
    pub effects: Option<String>,
    /// Any other tags (key -> value)
    pub other: std::collections::HashMap<String, String>,
}

impl DocBlock {
    /// Parse tags from the doc text.
    /// Extracts @param, @returns, @effects, and other @tag patterns.
    pub fn parse_tags(text: &str) -> DocTags {
        let mut tags = DocTags::default();
        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;
        
        while i < lines.len() {
            let line = lines[i].trim();
            
            // Match @param name description
            if let Some(param_text) = line.strip_prefix("@param") {
                let param_text = param_text.trim_start();
                if let Some(space_idx) = param_text.find(char::is_whitespace) {
                    let param_name = param_text[..space_idx].trim().to_string();
                    let description = param_text[space_idx..].trim().to_string();
                    if !param_name.is_empty() {
                        tags.params.insert(param_name, description);
                    }
                }
            }
            // Match @returns description
            else if let Some(returns_text) = line.strip_prefix("@returns") {
                let description = returns_text.trim_start().to_string();
                if !description.is_empty() {
                    tags.returns = Some(description);
                }
            }
            // Match @effects description
            else if let Some(effects_text) = line.strip_prefix("@effects") {
                let description = effects_text.trim_start().to_string();
                if !description.is_empty() {
                    tags.effects = Some(description);
                }
            }
            // Match any other @tag pattern
            else if line.starts_with('@') {
                let tag_text = line[1..].trim();
                if let Some(space_idx) = tag_text.find(char::is_whitespace) {
                    let tag_name = tag_text[..space_idx].trim().to_string();
                    let tag_value = tag_text[space_idx..].trim().to_string();
                    if !tag_name.is_empty() {
                        tags.other.insert(tag_name, tag_value);
                    }
                }
            }
            
            i += 1;
        }
        
        tags
    }
    
    /// Create a DocBlock from text and span, automatically parsing tags.
    pub fn new(span: Span, text: String) -> Self {
        let tags = Self::parse_tags(&text);
        Self { span, text, tags }
    }
    
    /// Create a DocBlock from documentation text (for native functions).
    /// Uses a zero span since native functions don't have source spans.
    pub fn from_text(text: &str) -> Self {
        Self::new(Span::new(0, 0), text.to_string())
    }
}

/// CST representation of a complete program.
#[derive(Debug, Clone, Serialize)]
pub struct CstProgram {
    pub blocks: Vec<Spanned<CstBlock>>,
}

/// CST representation of a block of statements.
#[derive(Debug, Clone, Serialize)]
pub struct CstBlock {
    pub statements: Vec<Spanned<CstStatement>>,
}

/// CST representation of statements (syntax-first, no semantic validation).
#[derive(Debug, Clone, Serialize)]
pub enum CstStatement {
    Mod {
        identifier: Spanned<String>,
    },
    Let {
        pub_keyword: Option<Span>, // span of "pub" if present
        let_keyword: Span,
        identifier: Spanned<String>,
        type_annotation: Option<Spanned<String>>, // includes the ":" and type
        eq: Span, // span of "="
        expression: Spanned<CstExpr>,
    },
    Const {
        pub_keyword: Option<Span>,
        const_keyword: Span,
        identifier: Spanned<String>,
        eq: Span,
        expression: Spanned<CstExpr>,
    },
    Assign {
        identifier: Spanned<String>,
        eq: Span,
        expression: Spanned<CstExpr>,
    },
    AssignIncrement {
        identifier: Spanned<String>,
        op: Span, // span of "+="
        expression: Spanned<CstExpr>,
    },
    AssignDecrement {
        identifier: Spanned<String>,
        op: Span, // span of "-="
        expression: Spanned<CstExpr>,
    },
    If {
        if_keyword: Span,
        arms: Vec<(Spanned<CstExpr>, Spanned<CstBlock>)>,
        else_keywords: Vec<Span>, // spans of "elseif" keywords
        else_keyword: Option<Span>,
        else_block: Option<Spanned<CstBlock>>,
    },
    Match {
        match_keyword: Span,
        expression: Spanned<CstExpr>,
        cases: Vec<(Option<Spanned<CstExpr>>, Spanned<CstBlock>)>, // None for wildcard
    },
    FunctionDeclaration {
        pub_keyword: Option<Span>,
        fn_keyword: Span,
        identifier: Spanned<String>,
        arguments: Vec<Spanned<CstArgument>>,
        return_type_arrow: Option<Spanned<ReturnTypeArrow>>, // -> or ~>
        body: Spanned<CstBlock>,
    },
    Return {
        return_keyword: Span,
        expression: Spanned<CstExpr>,
    },
    Loop {
        loop_keyword: Span,
        init_vars: Vec<(Spanned<String>, Span, Spanned<CstExpr>)>, // (var, =, expr)
        body: Spanned<CstBlock>,
    },
    While {
        while_keyword: Span,
        condition: Spanned<CstExpr>,
        body: Spanned<CstBlock>,
    },
    For {
        for_keyword: Span,
        var_name: Spanned<String>,
        in_keyword: Span,
        start: Spanned<CstExpr>,
        dotdot: Span, // span of ".."
        end: Spanned<CstExpr>,
        body: Spanned<CstBlock>,
    },
    Break {
        break_keyword: Span,
        expression: Option<Spanned<CstExpr>>,
    },
    Continue {
        continue_keyword: Span,
    },
    Use {
        use_keyword: Span,
        selector: Spanned<CstImportSelector>,
        from_keyword: Span,
        path: Vec<Spanned<String>>,
    },
    Struct {
        pub_keyword: Option<Span>,
        struct_keyword: Span,
        name: Spanned<String>,
        fields: Vec<Spanned<CstStructField>>,
    },
    Expression(Spanned<CstExpr>),
}

/// CST representation of function/closure arguments.
#[derive(Debug, Clone, Serialize)]
pub struct CstArgument {
    pub identifier: Spanned<String>,
    pub colon: Span,
    pub type_annotation: Spanned<String>,
}

/// CST representation of return type arrow (-> or ~>).
#[derive(Debug, Clone, Serialize)]
pub struct ReturnTypeArrow {
    pub arrow: Span, // span of "->" or "~>"
    pub is_effectful: bool, // true for ~>, false for ->
    pub type_annotation: Spanned<String>,
}

/// CST representation of import selector.
#[derive(Debug, Clone, Serialize)]
pub enum CstImportSelector {
    Single(Spanned<String>),
    Multiple(Vec<Spanned<String>>),
    Wildcard(Span), // span of "*"
}

/// CST representation of struct field.
#[derive(Debug, Clone, Serialize)]
pub struct CstStructField {
    pub name: Spanned<String>,
    pub colon: Span,
    pub type_annotation: Spanned<String>,
}

/// CST representation of expressions (syntax-first, no semantic validation).
#[derive(Debug, Clone, Serialize)]
pub enum CstExpr {
    Literal(Spanned<CstLiteral>),
    Identifier(Spanned<String>),
    FunctionCall {
        callee: Box<Spanned<CstExpr>>,
        open_paren: Span,
        arguments: Vec<Spanned<CstCallArgument>>,
        close_paren: Span,
    },
    PartialCall {
        func: Box<Spanned<CstExpr>>,
        open_paren: Span,
        args: Vec<Spanned<CstCallArgument>>,
        close_paren: Span,
    },
    Prefix {
        op: Spanned<CstUnaryOp>,
        rhs: Box<Spanned<CstExpr>>,
    },
    Postfix {
        lhs: Box<Spanned<CstExpr>>,
        op: Spanned<CstPostfixOp>,
    },
    Infix {
        lhs: Box<Spanned<CstExpr>>,
        op: Spanned<CstBinaryOp>,
        rhs: Box<Spanned<CstExpr>>,
    },
    Group {
        open_paren: Span,
        inner: Box<Spanned<CstExpr>>,
        close_paren: Span,
    },
    Loop {
        loop_keyword: Span,
        init_vars: Vec<(Spanned<String>, Span, Spanned<CstExpr>)>,
        body: Spanned<CstBlock>,
    },
    Compose {
        lhs: Box<Spanned<CstExpr>>,
        op: Spanned<CstComposeOp>,
        rhs: Box<Spanned<CstExpr>>,
    },
    MemberAccess {
        object: Box<Spanned<CstExpr>>,
        dots: Vec<Span>, // spans of "." operators
        members: Vec<Spanned<String>>,
    },
    StructInit {
        struct_name: Spanned<String>,
        open_brace: Span,
        fields: Vec<Spanned<CstStructInitField>>,
        close_brace: Span,
    },
    FieldAccess {
        object: Box<Spanned<CstExpr>>,
        dot: Span,
        field: Spanned<String>,
    },
    Array {
        open_bracket: Span,
        elements: Vec<Spanned<CstExpr>>,
        close_bracket: Span,
    },
    ArrayIndex {
        array: Box<Spanned<CstExpr>>,
        open_bracket: Span,
        indices: Vec<Spanned<CstIndexSpec>>,
        close_bracket: Span,
    },
    Closure {
        fn_keyword: Span,
        open_paren: Span,
        arguments: Vec<Spanned<CstClosureArg>>,
        close_paren: Span,
        return_type_arrow: Option<Spanned<ReturnTypeArrow>>,
        arrow: Option<Span>, // span of "=>" if present
        body: CstClosureBody,
    },
}

/// CST representation of literals.
#[derive(Debug, Clone, Serialize)]
pub enum CstLiteral {
    String(String),
    Number(f64),
    Boolean(bool),
}

/// CST representation of unary operators.
#[derive(Debug, Clone, Copy, Serialize)]
pub enum CstUnaryOp {
    Neg,
    Increment,
    Decrement,
    Not,
}

/// CST representation of postfix operators.
#[derive(Debug, Clone, Copy, Serialize)]
pub enum CstPostfixOp {
    Invoke, // !
}

/// CST representation of binary operators.
#[derive(Debug, Clone, Copy, Serialize)]
pub enum CstBinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
    And,
    Or,
}

/// CST representation of composition operators (|> or <|).
#[derive(Debug, Clone, Copy, Serialize)]
pub enum CstComposeOp {
    Forward,  // |>
    Reverse,  // <|
}

/// CST representation of call arguments (expression or hole).
#[derive(Debug, Clone, Serialize)]
pub enum CstCallArgument {
    Expr(Spanned<CstExpr>),
    Hole(Span), // span of "?"
}

/// CST representation of closure arguments.
#[derive(Debug, Clone, Serialize)]
pub struct CstClosureArg {
    pub identifier: Spanned<String>, // or placeholder "?"
    pub is_placeholder: bool,
    pub colon: Option<Span>,
    pub type_annotation: Option<Spanned<String>>,
}

/// CST representation of closure body.
#[derive(Debug, Clone, Serialize)]
pub enum CstClosureBody {
    Expression(Box<Spanned<CstExpr>>),
    Block(Spanned<CstBlock>),
}

/// CST representation of struct initialization fields.
#[derive(Debug, Clone, Serialize)]
pub struct CstStructInitField {
    pub name: Spanned<String>,
    pub colon: Span,
    pub value: Spanned<CstExpr>,
}

/// CST representation of array index specifications.
#[derive(Debug, Clone, Serialize)]
pub enum CstIndexSpec {
    Single(Spanned<CstExpr>),
    Range {
        start: Option<Spanned<CstExpr>>,
        dotdot: Span, // span of ".."
        end: Option<Spanned<CstExpr>>,
        step: Option<(Span, Spanned<CstExpr>)>, // (.., expr)
    },
    InclusiveRange {
        start: Option<Spanned<CstExpr>>,
        dotdoteq: Span, // span of "..="
        end: Option<Spanned<CstExpr>>,
    },
}

