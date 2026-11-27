//! calc()/min()/max()/clamp() symbolic handling (directive §9.1).
//!
//! Expressions parse into a [`CalcExpr`] tree with correct operator
//! precedence (`*`/`/` bind tighter than `+`/`-`), constants fold, and
//! percentages stay symbolic until [`CalcExpr::evaluate`] receives the
//! reference the percentage applies against. Division by zero and type
//! mismatches (a percentage multiplied by a percentage) are errors, not
//! guesses, so callers can report `CASA-GEO-002`.

use super::{Context, Px};
use goya_css_syntax::{parse_component_values, ComponentValue, Token};

/// One operand kind inside a calc expression. Percentages are the only
/// symbolic leaves; every dimension resolves to pixels at parse time.
#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    Px(f64),
    Percent(f64),
}

impl Operand {
    /// Resolve to pixels, percentages against `reference`.
    fn to_px(&self, reference: Px) -> Option<f64> {
        match *self {
            Operand::Px(px) => Some(px),
            Operand::Percent(percent) => Some(percent / 100.0 * reference),
        }
    }
}

/// A parsed math expression: leaves, arithmetic, and the math functions.
#[derive(Debug, Clone, PartialEq)]
pub enum CalcExpr {
    Leaf(Operand),
    Sum(Vec<CalcExpr>),
    /// `a * b` / `a / b`; the divisor must evaluate to a plain number.
    Product {
        left: Box<CalcExpr>,
        right: Box<CalcExpr>,
    },
    Divide {
        dividend: Box<CalcExpr>,
        divisor: Box<CalcExpr>,
    },
    Min(Vec<CalcExpr>),
    Max(Vec<CalcExpr>),
    Clamp {
        minimum: Box<CalcExpr>,
        value: Box<CalcExpr>,
        maximum: Box<CalcExpr>,
    },
}

/// Parse a `calc()`/`min()`/`max()`/`clamp()` value (or a bare length)
/// into an expression tree. Returns `None` for malformed input.
pub fn parse(value: &str, context: &Context) -> Option<CalcExpr> {
    let values = parse_component_values(value);
    let mut parser = ExprParser {
        values: &values,
        position: 0,
        context,
    };
    // Top level: exactly one operand or math function. Arithmetic only
    // exists inside calc()/min()/max()/clamp() or parentheses; a bare
    // `4px + 2px` property value is not valid CSS math.
    let expression = parser.parse_factor()?;
    if parser.position != parser.values.len() {
        return None;
    }
    Some(expression)
}

/// Parse and fully evaluate against a percentage reference.
pub fn evaluate_value(value: &str, reference: Px, context: &Context) -> Option<Px> {
    parse(value, context)?.evaluate(reference)
}

impl CalcExpr {
    /// Evaluate to CSS reference pixels; percentages resolve against
    /// `reference`. Returns `None` on division by zero.
    pub fn evaluate(&self, reference: Px) -> Option<Px> {
        match self {
            CalcExpr::Leaf(operand) => operand.to_px(reference),
            CalcExpr::Sum(terms) => {
                let mut total = 0.0;
                for term in terms {
                    total += term.evaluate(reference)?;
                }
                Some(total)
            }
            CalcExpr::Product { left, right } => {
                // Multiplying two percentages has no defined unit.
                if matches!(**left, CalcExpr::Leaf(Operand::Percent(_)))
                    && matches!(**right, CalcExpr::Leaf(Operand::Percent(_)))
                {
                    return None;
                }
                Some(left.evaluate(reference)? * right.evaluate(reference)?)
            }
            CalcExpr::Divide { dividend, divisor } => {
                let divisor = divisor.evaluate(reference)?;
                if divisor == 0.0 {
                    return None;
                }
                Some(dividend.evaluate(reference)? / divisor)
            }
            CalcExpr::Min(terms) => {
                let mut values = Vec::with_capacity(terms.len());
                for term in terms {
                    values.push(term.evaluate(reference)?);
                }
                values.iter().copied().reduce(f64::min)
            }
            CalcExpr::Max(terms) => {
                let mut values = Vec::with_capacity(terms.len());
                for term in terms {
                    values.push(term.evaluate(reference)?);
                }
                values.iter().copied().reduce(f64::max)
            }
            CalcExpr::Clamp {
                minimum,
                value,
                maximum,
            } => {
                let value = value.evaluate(reference)?;
                let minimum = minimum.evaluate(reference)?;
                let maximum = maximum.evaluate(reference)?;
                Some(value.max(minimum).min(maximum))
            }
        }
    }
}

/// Recursive-descent parser over the component values of one math
/// expression: `sum := product (('+'|'-') product)*`,
/// `product := factor (('*'|'/') factor)*`.
struct ExprParser<'a> {
    values: &'a [ComponentValue],
    position: usize,
    context: &'a Context,
}

impl<'a> ExprParser<'a> {
    fn peek(&self) -> Option<&'a ComponentValue> {
        self.values.get(self.position)
    }

    fn skip_whitespace(&mut self) {
        while matches!(
            self.peek(),
            Some(ComponentValue::Token(Token::Whitespace, _))
        ) {
            self.position += 1;
        }
    }

    fn parse_sum(&mut self) -> Option<CalcExpr> {
        let mut terms = vec![self.parse_product()?];
        loop {
            self.skip_whitespace();
            let operator = match self.peek() {
                Some(ComponentValue::Token(Token::Delim('+'), _)) => 1.0,
                Some(ComponentValue::Token(Token::Delim('-'), _)) => -1.0,
                _ => {
                    return Some(if terms.len() == 1 {
                        terms.pop().unwrap()
                    } else {
                        CalcExpr::Sum(terms)
                    })
                }
            };
            self.position += 1;
            self.skip_whitespace();
            let term = self.parse_product()?;
            terms.push(if operator < 0.0 {
                CalcExpr::Product {
                    left: Box::new(CalcExpr::Leaf(Operand::Px(-1.0))),
                    right: Box::new(term),
                }
            } else {
                term
            });
        }
    }

    fn parse_product(&mut self) -> Option<CalcExpr> {
        let mut left = self.parse_factor()?;
        loop {
            self.skip_whitespace();
            let divide = match self.peek() {
                Some(ComponentValue::Token(Token::Delim('*'), _)) => false,
                Some(ComponentValue::Token(Token::Delim('/'), _)) => true,
                _ => return Some(left),
            };
            self.position += 1;
            self.skip_whitespace();
            let right = self.parse_factor()?;
            left = if divide {
                CalcExpr::Divide {
                    dividend: Box::new(left),
                    divisor: Box::new(right),
                }
            } else {
                CalcExpr::Product {
                    left: Box::new(left),
                    right: Box::new(right),
                }
            };
        }
    }

    fn parse_factor(&mut self) -> Option<CalcExpr> {
        self.skip_whitespace();
        match self.peek()? {
            ComponentValue::Function {
                name, arguments, ..
            } => {
                let name = name.to_ascii_lowercase();
                self.position += 1;
                let mut parser = ExprParser {
                    values: arguments,
                    position: 0,
                    context: self.context,
                };
                match name.as_str() {
                    "calc" => parser.parse_sum(),
                    "min" | "max" => {
                        let mut terms = Vec::new();
                        loop {
                            terms.push(parser.parse_sum()?);
                            parser.skip_whitespace();
                            match parser.peek() {
                                Some(ComponentValue::Token(Token::Comma, _)) => {
                                    parser.position += 1;
                                }
                                _ => break,
                            }
                        }
                        if parser.position != parser.values.len() {
                            return None;
                        }
                        Some(if name == "min" {
                            CalcExpr::Min(terms)
                        } else {
                            CalcExpr::Max(terms)
                        })
                    }
                    "clamp" => {
                        let minimum = parser.parse_sum()?;
                        parser.skip_whitespace();
                        if !matches!(parser.peek(), Some(ComponentValue::Token(Token::Comma, _))) {
                            return None;
                        }
                        parser.position += 1;
                        let value = parser.parse_sum()?;
                        parser.skip_whitespace();
                        if !matches!(parser.peek(), Some(ComponentValue::Token(Token::Comma, _))) {
                            return None;
                        }
                        parser.position += 1;
                        let maximum = parser.parse_sum()?;
                        if parser.position != parser.values.len() {
                            return None;
                        }
                        Some(CalcExpr::Clamp {
                            minimum: Box::new(minimum),
                            value: Box::new(value),
                            maximum: Box::new(maximum),
                        })
                    }
                    _ => None,
                }
            }
            ComponentValue::Block(block) if block.open == '(' => {
                // Parenthesised group: arithmetic is allowed inside
                // without an explicit calc().
                self.position += 1;
                let mut parser = ExprParser {
                    values: &block.contents,
                    position: 0,
                    context: self.context,
                };
                let expression = parser.parse_sum()?;
                if parser.position != parser.values.len() {
                    return None;
                }
                Some(expression)
            }
            ComponentValue::Token(Token::Number(number), _) => {
                self.position += 1;
                Some(CalcExpr::Leaf(Operand::Px(number.value)))
            }
            ComponentValue::Token(Token::Percentage(number), _) => {
                self.position += 1;
                Some(CalcExpr::Leaf(Operand::Percent(number.value)))
            }
            ComponentValue::Token(Token::Dimension { number, unit }, _) => {
                self.position += 1;
                let resolved = super::dimensional::resolve_value(
                    &format!("{}{}", number.value, unit),
                    0.0,
                    self.context,
                );
                resolved.map(|px| CalcExpr::Leaf(Operand::Px(px)))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> Context {
        Context {
            font_size: 16.0,
            root_font_size: 16.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
        }
    }

    #[test]
    fn plain_arithmetic_evaluates() {
        let ctx = context();
        assert_eq!(evaluate_value("calc(4px + 3px)", 0.0, &ctx), Some(7.0));
        assert_eq!(evaluate_value("calc(10px - 4px * 2)", 0.0, &ctx), Some(2.0));
        assert_eq!(evaluate_value("calc(8px / (1 + 1))", 0.0, &ctx), Some(4.0));
        assert_eq!(evaluate_value("calc(2 * 3px + 4px)", 0.0, &ctx), Some(10.0));
    }

    #[test]
    fn percentages_stay_symbolic_until_reference() {
        let ctx = context();
        let expression = parse("calc(50% + 10px)", &ctx).unwrap();
        // No reference yet: parse succeeds, evaluation needs one.
        assert_eq!(expression.evaluate(200.0), Some(110.0));
        assert_eq!(expression.evaluate(100.0), Some(60.0));
    }

    #[test]
    fn min_max_clamp_and_nesting() {
        let ctx = context();
        assert_eq!(
            evaluate_value("min(30px, 20px, 25px)", 0.0, &ctx),
            Some(20.0)
        );
        assert_eq!(evaluate_value("max(30px, 20px)", 0.0, &ctx), Some(30.0));
        assert_eq!(
            evaluate_value("clamp(10px, 50px, 40px)", 0.0, &ctx),
            Some(40.0)
        );
        assert_eq!(
            evaluate_value("calc(max(10px, 5px) + 1px)", 0.0, &ctx),
            Some(11.0)
        );
        // Units resolve inside expressions.
        assert_eq!(evaluate_value("calc(1em + 4px)", 0.0, &ctx), Some(20.0));
    }

    #[test]
    fn division_by_zero_and_malformed_are_errors() {
        let ctx = context();
        assert_eq!(evaluate_value("calc(4px / 0)", 0.0, &ctx), None);
        assert_eq!(evaluate_value("calc(4px +)", 0.0, &ctx), None);
        assert_eq!(evaluate_value("calc(auto + 4px)", 0.0, &ctx), None);
        // Trailing junk after a complete expression is rejected, and
        // arithmetic outside a math function is not CSS math at all.
        assert_eq!(evaluate_value("calc(4px) + 2px", 0.0, &ctx), None);
        assert_eq!(evaluate_value("4px + 2px", 0.0, &ctx), None);
    }
}
