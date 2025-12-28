/// Represents an expression in the CantaLoop language.
/// 
/// Expressions are the building blocks of computation, including
/// literals, variables, function calls, and operations.
#[derive(Debug, Clone)]
pub enum Expression {
    Literal(Literal),
    Identifier(String),
    FunctionCall {
        callee: Box<Expression>, // Can be Identifier, Compose, or any expression that evaluates to a callable
        arguments: Vec<Expression>,
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
}

/// Unary operators that operate on a single expression.
#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Neg,
    Increment,
    Decrement,
    Not,
}

#[derive(Debug, Clone, Copy)]
pub enum PostfixOp {
    Invoke,
}

/// Binary operators that operate on two expressions.
#[derive(Debug, Clone, Copy)]
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
#[derive(Debug)]
pub struct Program {
    pub blocks: Vec<Block>
}

/// A block of statements executed sequentially.
/// 
/// Blocks create a new scope for variable declarations.
#[derive(Debug, Clone)]
pub struct Block {
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub struct Argument {
    pub identifier: String,
    pub kind: String
}

/// Represents a statement in the CantaLoop language.
/// 
/// Statements are the top-level constructs that perform actions:
/// variable declarations, assignments, control flow, function definitions.
#[derive(Debug, Clone)]
pub enum Statement {
    Let {
        identifier: String,
        type_annotation: Option<String>,
        expression: Expression,
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
        identifier: String,
        arguments: Vec<Argument>,
        return_type: Option<String>,
        body: Block
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
    Expression(Expression),
}

/// Literal values in source code: numbers, strings, and booleans.
#[derive(Debug, Clone)]
pub enum Literal {
    String(String),
    Number(f64),
    Boolean(bool)
}
impl Literal {
    pub(crate) fn to_string(&self) -> String {
        match self {
            Literal::Number(val) => format!("Number({})", val),
            Literal::String(val) => format!("String({})", val),
            Literal::Boolean(val) => format!("Bool({})", val),
        }
    }
}