#[derive(Debug, Clone)]
pub enum Expression {
    Literal(Literal),
    Identifier(String),
    FunctionCall {
        identifier: String,
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
}

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

#[derive(Debug, Clone, Copy)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
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

#[derive(Debug)]
pub struct Program {
    pub blocks: Vec<Block>
}

#[derive(Debug)]
pub struct Block {
    pub statements: Vec<Statement>,
}

#[derive(Debug)]
pub struct Argument {
    pub identifier: String,
    pub kind: String
}

#[derive(Debug)]
pub enum Statement {
    Let {
        identifier: String,
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
    FunctionDeclaration {
        identifier: String,
        arguments: Vec<Argument>,
        body: Block
    },
    Return {
        expression: Expression,
    },
    Expression(Expression),
}

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