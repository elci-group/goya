//! CSS transform list parsing for the appearance transformation stage
//! (directive GOYA-DIS-CASA-001).

use super::{angle_degrees, length, meaningful, number, percentage};
use crate::geometry::Length;
use goya_css_syntax::{parse_component_values, ComponentValue, Token};

/// One 2D transform function. Translations keep percentages symbolic;
/// the transform box is unknown at this stage. 3D functions are not
/// modeled and parse as `None` rather than being flattened silently.
#[derive(Debug, Clone, PartialEq)]
pub enum TransformOp {
    Translate {
        x: Length,
        y: Length,
    },
    Scale {
        x: f64,
        y: f64,
    },
    Rotate {
        degrees: f64,
    },
    Skew {
        x_degrees: f64,
        y_degrees: f64,
    },
    Matrix {
        a: f64,
        b: f64,
        c: f64,
        d: f64,
        e: f64,
        f: f64,
    },
}

/// Parse one computed `transform` value: a whitespace-separated list of
/// transform functions, or `none` for the empty list.
pub fn parse(value: &str) -> Option<Vec<TransformOp>> {
    let values = parse_component_values(value);
    let meaningful = meaningful(&values);
    match meaningful.as_slice() {
        [] => None,
        [ComponentValue::Token(Token::Ident(name), _)] if name.eq_ignore_ascii_case("none") => {
            Some(Vec::new())
        }
        _ => {
            let mut operations = Vec::with_capacity(meaningful.len());
            for value in &meaningful {
                let ComponentValue::Function {
                    name, arguments, ..
                } = value
                else {
                    return None;
                };
                operations.push(function(name, arguments)?);
            }
            Some(operations)
        }
    }
}

fn function(name: &str, arguments: &[ComponentValue]) -> Option<TransformOp> {
    // Transform function arguments are comma-separated (legacy) or, for
    // single-argument functions, bare; commas never carry meaning here.
    let tokens: Vec<&ComponentValue> = meaningful(arguments)
        .into_iter()
        .filter(|value| !matches!(value, ComponentValue::Token(Token::Comma, _)))
        .collect();
    let name = name.to_ascii_lowercase();
    match name.as_str() {
        "translate" => {
            // A missing y argument is an absolute zero, not a copy of the
            // x argument (which may be a percentage).
            let [x, y] = one_or_two(&tokens, length, Length::Px(0.0))?;
            Some(TransformOp::Translate { x, y })
        }
        "translatex" => Some(TransformOp::Translate {
            x: length(single(&tokens)?)?,
            y: Length::Px(0.0),
        }),
        "translatey" => Some(TransformOp::Translate {
            x: Length::Px(0.0),
            y: length(single(&tokens)?)?,
        }),
        "scale" => {
            // scale(s) scales both axes by s; scale(x, y) is explicit.
            let x = factor(*tokens.first()?)?;
            let y = match tokens.len() {
                1 => x,
                2 => factor(tokens[1])?,
                _ => return None,
            };
            Some(TransformOp::Scale { x, y })
        }
        "scalex" => Some(TransformOp::Scale {
            x: factor(single(&tokens)?)?,
            y: 1.0,
        }),
        "scaley" => Some(TransformOp::Scale {
            x: 1.0,
            y: factor(single(&tokens)?)?,
        }),
        "rotate" => Some(TransformOp::Rotate {
            degrees: angle_degrees(single(&tokens)?)?,
        }),
        "skew" => {
            let [x_degrees, y_degrees] = one_or_two(&tokens, angle_degrees, 0.0)?;
            Some(TransformOp::Skew {
                x_degrees,
                y_degrees,
            })
        }
        "skewx" => Some(TransformOp::Skew {
            x_degrees: angle_degrees(single(&tokens)?)?,
            y_degrees: 0.0,
        }),
        "skewy" => Some(TransformOp::Skew {
            x_degrees: 0.0,
            y_degrees: angle_degrees(single(&tokens)?)?,
        }),
        "matrix" => {
            let [a, b, c, d, e, f] = exact(&tokens, number)?;
            Some(TransformOp::Matrix { a, b, c, d, e, f })
        }
        _ => None,
    }
}

fn single<'a>(tokens: &'a [&'a ComponentValue]) -> Option<&'a ComponentValue> {
    match tokens {
        [only] => Some(only),
        _ => None,
    }
}

/// Parse one or two tokens; a single token pairs the parsed value with
/// `default` (the CSS default for the second argument).
fn one_or_two<T: Copy>(
    tokens: &[&ComponentValue],
    parse: impl Fn(&ComponentValue) -> Option<T>,
    default: T,
) -> Option<[T; 2]> {
    match tokens.len() {
        1 => Some([parse(tokens[0])?, default]),
        2 => Some([parse(tokens[0])?, parse(tokens[1])?]),
        _ => None,
    }
}

fn exact<T, const N: usize>(
    tokens: &[&ComponentValue],
    parse: impl Fn(&ComponentValue) -> Option<T>,
) -> Option<[T; N]> {
    if tokens.len() != N {
        return None;
    }
    let mut parsed = Vec::with_capacity(N);
    for token in tokens {
        parsed.push(parse(token)?);
    }
    parsed.try_into().ok()
}

/// A scale factor: a plain number or a percentage (100% = 1).
fn factor(value: &ComponentValue) -> Option<f64> {
    number(value).or_else(|| percentage(value).map(|percent| percent / 100.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_chain_parses_in_order() {
        let operations = parse("translateX(10px) rotate(45deg)").unwrap();
        assert_eq!(operations.len(), 2);
        assert_eq!(
            operations[0],
            TransformOp::Translate {
                x: Length::Px(10.0),
                y: Length::Px(0.0)
            }
        );
        assert_eq!(operations[1], TransformOp::Rotate { degrees: 45.0 });
    }

    #[test]
    fn translate_defaults_y_to_zero() {
        let operations = parse("translate(10px)").unwrap();
        assert_eq!(
            operations[0],
            TransformOp::Translate {
                x: Length::Px(10.0),
                y: Length::Px(0.0)
            }
        );
        let operations = parse("translate(10px, 20%)").unwrap();
        assert_eq!(
            operations[0],
            TransformOp::Translate {
                x: Length::Px(10.0),
                y: Length::Percent(20.0)
            }
        );
    }

    #[test]
    fn scale_accepts_numbers_and_percentages() {
        assert_eq!(
            parse("scale(2)").unwrap()[0],
            TransformOp::Scale { x: 2.0, y: 2.0 }
        );
        assert_eq!(
            parse("scale(50%, 200%)").unwrap()[0],
            TransformOp::Scale { x: 0.5, y: 2.0 }
        );
        assert_eq!(
            parse("scaleX(3)").unwrap()[0],
            TransformOp::Scale { x: 3.0, y: 1.0 }
        );
    }

    #[test]
    fn skew_defaults_y_to_zero_and_matrix_parses() {
        assert_eq!(
            parse("skew(10deg)").unwrap()[0],
            TransformOp::Skew {
                x_degrees: 10.0,
                y_degrees: 0.0
            }
        );
        assert_eq!(
            parse("skew(10deg, 20deg)").unwrap()[0],
            TransformOp::Skew {
                x_degrees: 10.0,
                y_degrees: 20.0
            }
        );
        assert_eq!(
            parse("matrix(1, 0, 0, 1, 10, 20)").unwrap()[0],
            TransformOp::Matrix {
                a: 1.0,
                b: 0.0,
                c: 0.0,
                d: 1.0,
                e: 10.0,
                f: 20.0
            }
        );
    }

    #[test]
    fn none_is_the_empty_list() {
        assert_eq!(parse("none"), Some(Vec::new()));
    }

    #[test]
    fn malformed_transforms_are_rejected() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("rotate(45deg) 10px"), None);
        assert_eq!(parse("rotate(45)"), None);
        assert_eq!(parse("translate()"), None);
        assert_eq!(parse("matrix(1, 0, 0, 1, 10)"), None);
        assert_eq!(parse("translate3d(1px, 2px, 3px)"), None);
        assert_eq!(parse("scaleX(2px)"), None);
        assert_eq!(parse("wobble(5px)"), None);
    }
}
