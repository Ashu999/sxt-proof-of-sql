use super::{ExpressionEvaluationError, ExpressionEvaluationResult};
use crate::base::{
    database::{OwnedColumn, OwnedTable},
    math::decimal::{try_into_to_scalar, Precision},
    scalar::Scalar,
};
use proof_of_sql_parser::{
    intermediate_ast::{BinaryOperator, Expression, Literal, UnaryOperator},
    Identifier,
};

impl<S: Scalar> OwnedTable<S> {
    /// Evaluate an expression on the table.
    pub fn evaluate(&self, expr: &Expression) -> ExpressionEvaluationResult<OwnedColumn<S>> {
        match expr {
            Expression::Column(identifier) => self.evaluate_column(identifier),
            Expression::Literal(lit) => self.evaluate_literal(lit),
            Expression::Binary { op, left, right } => self.evaluate_binary_expr(*op, left, right),
            Expression::Unary { op, expr } => self.evaluate_unary_expr(*op, expr),
            _ => Err(ExpressionEvaluationError::Unsupported {
                expression: format!("Expression {:?} is not supported yet", expr),
            }),
        }
    }

    fn evaluate_column(
        &self,
        identifier: &Identifier,
    ) -> ExpressionEvaluationResult<OwnedColumn<S>> {
        Ok(self
            .inner_table()
            .get(identifier)
            .ok_or(ExpressionEvaluationError::ColumnNotFound {
                error: identifier.to_string(),
            })?
            .clone())
    }

    fn evaluate_literal(&self, lit: &Literal) -> ExpressionEvaluationResult<OwnedColumn<S>> {
        let len = self.num_rows();
        match lit {
            Literal::Boolean(b) => Ok(OwnedColumn::Boolean(vec![*b; len])),
            Literal::BigInt(i) => Ok(OwnedColumn::BigInt(vec![*i; len])),
            Literal::Int128(i) => Ok(OwnedColumn::Int128(vec![*i; len])),
            Literal::Decimal(d) => {
                let scale = d.scale();
                let precision = Precision::new(d.precision())?;
                let scalar = try_into_to_scalar(d, precision, scale)?;
                Ok(OwnedColumn::Decimal75(precision, scale, vec![scalar; len]))
            }
            Literal::VarChar(s) => Ok(OwnedColumn::VarChar(vec![s.clone(); len])),
            Literal::Timestamp(its) => Ok(OwnedColumn::TimestampTZ(
                its.timeunit(),
                its.timezone(),
                vec![its.timestamp().timestamp(); len],
            )),
        }
    }

    fn evaluate_unary_expr(
        &self,
        op: UnaryOperator,
        expr: &Expression,
    ) -> ExpressionEvaluationResult<OwnedColumn<S>> {
        let column = self.evaluate(expr)?;
        match op {
            UnaryOperator::Not => Ok(column.element_wise_not()?),
        }
    }

    fn evaluate_binary_expr(
        &self,
        op: BinaryOperator,
        left: &Expression,
        right: &Expression,
    ) -> ExpressionEvaluationResult<OwnedColumn<S>> {
        let left = self.evaluate(left)?;
        let right = self.evaluate(right)?;
        match op {
            BinaryOperator::And => Ok(left.element_wise_and(&right)?),
            BinaryOperator::Or => Ok(left.element_wise_or(&right)?),
            BinaryOperator::Equal => Ok(left.element_wise_eq(&right)?),
            BinaryOperator::GreaterThanOrEqual => Ok(left.element_wise_ge(&right)?),
            BinaryOperator::LessThanOrEqual => Ok(left.element_wise_le(&right)?),
            BinaryOperator::Add => Ok((left + right)?),
            BinaryOperator::Subtract => Ok((left - right)?),
            BinaryOperator::Multiply => Ok((left * right)?),
            BinaryOperator::Division => Ok((left / right)?),
        }
    }
}
