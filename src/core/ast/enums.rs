use serde::Serialize;
use crate::core::cst::CstId;

/// Represents a placeholder hole in partial application.
#[derive(Debug, Clone, Serialize)]
pub struct Hole;

/// Represents an identifier with its CST identity.
/// 
/// Phase 3: AST nodes carry CstId to enable identity tracking through lowering.
/// This allows binding CST nodes to HIR symbols for LSP semantic features.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Hash)]
pub struct AstIdent {
    pub name: String,
    pub cst_id: CstId,
}

impl std::fmt::Display for AstIdent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl std::borrow::Borrow<str> for AstIdent {
    fn borrow(&self) -> &str {
        &self.name
    }
}

/// Represents an expression in the CantaLoop language.
/// 
/// Expressions are the building blocks of computation, including
/// literals, variables, function calls, and operations.
#[derive(Debug, Clone, Serialize)]
pub enum Expression {
    Literal(Literal),
    Identifier(AstIdent), // Phase 3: Identifier carries CstId for identity tracking
    FunctionCall {
        callee: Box<Expression>, // Can be Identifier, Compose, or any expression that evaluates to a callable
        arguments: Vec<Expression>,
        cst_id: CstId, // Phase 3: FunctionCall carries CstId for identity tracking
    },
    PartialCall {
        func: Box<Expression>, // Function to partially apply
        args: Vec<CallArgument>, // Arguments with holes
    },
    Prefix {
        op: UnaryOp,
        rhs: Box<Expression>,
    },
    Postfix {
        lhs: Box<Expression>,
        op: PostfixOp,
        args: Option<Vec<Expression>>, // Optional arguments for invoke: add5!(10)
    },
    Infix {
        lhs: Box<Expression>,
        op: BinaryOp,
        rhs: Box<Expression>,
    },
    #[allow(dead_code)]
    Group(Box<Expression>),
    Loop {
        init_vars: Vec<(String, Expression)>, // (variable_name, initial_value)
        body: Block,
    },
    Compose {
        lhs: Box<Expression>,
        rhs: Box<Expression>,
        reverse: bool, // true for <|, false for |>
    },
    MemberAccess {
        object: Box<Expression>, // e.g., "utils" in "utils.add"
        member: AstIdent, // e.g., "add" in "utils.add" - Phase 3: member carries CstId
        cst_id: CstId, // Phase 3: MemberAccess carries CstId for identity tracking
    },
    StructInit {
        struct_name: AstIdent, // e.g., "Point" in "Point { x: 10, y: 20 }" - Phase 3: struct_name carries CstId
        fields: Vec<(AstIdent, Expression)>, // (field_name, value) - Phase 3: field_name carries CstId
        cst_id: CstId, // Phase 3: StructInit carries CstId for identity tracking
    },
    FieldAccess {
        object: Box<Expression>, // e.g., "p" in "p.x"
        field: AstIdent, // e.g., "x" in "p.x" - Phase 3: field carries CstId
        cst_id: CstId, // Phase 3: FieldAccess carries CstId for identity tracking
    },
    Array(Vec<Expression>), // Array literal: [expr, expr, ...]
    ArrayIndex {
        array: Box<Expression>, // The array being indexed
        indices: Vec<IndexSpec>, // Index specifications (supports multi-dimensional)
    },
    Closure {
        arguments: Vec<Argument>, // Function parameters
        return_type: Option<String>, // Optional return type annotation
        body: ClosureBody, // Either an expression or a block
    },
}

/// Represents an array index specification.
#[derive(Debug, Clone, Serialize)]
pub enum IndexSpec {
    /// Single index: arr[3] or arr[-1]
    Single(Expression),
    /// Range (inclusive start, exclusive end): arr[1..5]
    Range {
        start: Option<Expression>, // None means from start
        end: Option<Expression>,   // None means to end
        step: Option<Expression>,  // None means step of 1
    },
    /// Inclusive range (both inclusive): arr[1..=5]
    InclusiveRange {
        start: Option<Expression>,
        end: Option<Expression>,
    },
}

/// Unary operators that operate on a single expression.
#[derive(Debug, Clone, Copy, Serialize)]
pub enum UnaryOp {
    Neg,
    Increment,
    Decrement,
    Not,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum PostfixOp {
    Invoke,
}

/// Binary operators that operate on two expressions.
#[derive(Debug, Clone, Copy, Serialize)]
pub enum BinaryOp {
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

/// Represents a complete CantaLoop program.
/// 
/// A program consists of one or more blocks, which are executed sequentially.
#[derive(Debug, Clone, Serialize)]
pub struct Program {
    pub blocks: Vec<Block>
}

/// A block of statements executed sequentially.
/// 
/// Blocks create a new scope for variable declarations.
#[derive(Debug, Clone, Serialize)]
pub struct Block {
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Argument {
    pub identifier: AstIdent, // Phase 3: identifier carries CstId for identity tracking
    pub kind: String
}

/// Represents an argument in a function call - either an expression or a hole placeholder.
#[derive(Debug, Clone, Serialize)]
pub enum CallArgument {
    Expr(Expression),
    Hole,
}

/// Represents the body of a closure - either a single expression or a block.
#[derive(Debug, Clone, Serialize)]
pub enum ClosureBody {
    Expression(Box<Expression>), // Single expression: fn(x) => x + 1
    Block(Block),                 // Block: fn(x) => { return x + 1; }
}

/// Represents a statement in the CantaLoop language.
/// 
/// Statements are the top-level constructs that perform actions:
/// variable declarations, assignments, control flow, function definitions.
#[derive(Debug, Clone, Serialize)]
pub enum Statement {
    Mod {
        identifier: String,
    },
    Let {
        identifier: AstIdent, // Phase 3: identifier carries CstId for identity tracking
        type_annotation: Option<String>,
        expression: Expression,
        pub_visibility: bool,
    },
    Const {
        identifier: AstIdent, // Phase 3: identifier carries CstId for identity tracking
        expression: Expression,
        pub_visibility: bool,
    },
    Assign {
        identifier: String,
        expression: Expression,
    },
    AssignIncrement {
        identifier: String,
        expression: Expression,
    },
    AssignDecrement {
        identifier: String,
        expression: Expression,
    },
    If {
        arms: Vec<(Expression, Block)>,
        else_block: Option<Block>
    },
    Match {
        expression: Expression,
        cases: Vec<(Option<Expression>, Block)>, // (pattern expression, block) - None for wildcard
    },
    FunctionDeclaration {
        identifier: AstIdent, // Phase 3: identifier carries CstId for identity tracking
        arguments: Vec<Argument>,
        return_type: Option<String>,
        body: Block,
        pub_visibility: bool,
        cst_id: CstId, // Phase 3: FunctionDeclaration carries CstId for identity tracking
    },
    Return {
        expression: Expression,
    },
    Loop {
        init_vars: Vec<(String, Expression)>, // (variable_name, initial_value)
        body: Block,
    },
    While {
        condition: Expression,
        body: Block,
    },
    For {
        var_name: String,
        start: Expression,
        end: Expression,
        body: Block,
    },
    Break {
        expression: Option<Expression>,
    },
    Continue,
    Use {
        path: Vec<String>, // Dot-separated path like ["math", "utils"]
        selector: ImportSelector,
    },
    Struct {
        name: String,
        fields: Vec<(String, String)>, // (field_name, type_annotation)
        pub_visibility: bool,
    },
    Expression(Expression),
}

/// Represents what to import from a module path.
#[derive(Debug, Clone, Serialize)]
pub enum ImportSelector {
    /// Import a single name: `use math.utils.square;`
    Single(String),
    /// Import multiple names: `use math.utils.{cube, pow};`
    Multiple(Vec<String>),
    /// Import all: `use math.utils.*;`
    Wildcard,
}

/// Literal values in source code: numbers, strings, and booleans.
#[derive(Debug, Clone, Serialize)]
pub enum Literal {
    String(String),
    Number(f64),
    Boolean(bool)
}

impl std::fmt::Display for Literal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Literal::Number(val) => write!(f, "Number({})", val),
            Literal::String(val) => write!(f, "String({})", val),
            Literal::Boolean(val) => write!(f, "Bool({})", val),
        }
    }
}

