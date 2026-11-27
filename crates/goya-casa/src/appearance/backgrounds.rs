//! CSS background shorthand parsing for the appearance transformation
//! stage (directive GOYA-DIS-CASA-001).

use super::colors::{self, Color};
use super::gradients::{self, Gradient};
use super::{meaningful, split_comma};
use goya_css_syntax::{parse_component_values, ComponentValue, Token};

/// One paintable background layer: a gradient or an image URL. Sizing,
/// positioning and repetition clauses are not modeled; their known
/// keywords and lengths are accepted and skipped so common shorthands
/// still resolve to their paint layers.
#[derive(Debug, Clone, PartialEq)]
pub enum BackgroundImage {
    Gradient(Gradient),
    Url(String),
}

/// A parsed `background` value: the paint layers in CSS order (first is
/// painted on top) plus at most one background color. An empty value or
/// an unknown component is `None`.
#[derive(Debug, Clone, PartialEq)]
pub struct Background {
    pub color: Option<Color>,
    pub layers: Vec<BackgroundImage>,
}

/// Parse one computed `background` value: a comma-separated layer list,
/// where the final layer may also carry the background color.
pub fn parse(value: &str) -> Option<Background> {
    let values = parse_component_values(value);
    let layers = split_comma(&values);
    if layers.iter().all(|layer| meaningful(layer).is_empty()) {
        return None;
    }
    let mut color = None;
    let mut images = Vec::new();
    for layer in &layers {
        parse_layer(layer, &mut color, &mut images)?;
    }
    Some(Background {
        color,
        layers: images,
    })
}

fn parse_layer(
    layer: &[ComponentValue],
    color: &mut Option<Color>,
    images: &mut Vec<BackgroundImage>,
) -> Option<()> {
    for value in meaningful(layer) {
        match value {
            ComponentValue::Function { name, .. } => {
                if name.to_ascii_lowercase().ends_with("-gradient") {
                    images.push(BackgroundImage::Gradient(gradients::from_component(value)?));
                } else if color.replace(colors::from_component(value)?).is_some() {
                    // At most one color in the whole shorthand.
                    return None;
                }
            }
            ComponentValue::Token(
                Token::Url {
                    value: url,
                    terminated: true,
                },
                _,
            ) => {
                images.push(BackgroundImage::Url(url.clone()));
            }
            ComponentValue::Token(Token::Ident(name), _) => {
                let name = name.to_ascii_lowercase();
                if known_keyword(&name) {
                    continue;
                }
                if color.replace(colors::from_component(value)?).is_some() {
                    return None;
                }
            }
            ComponentValue::Token(Token::Hash { .. }, _) => {
                if color.replace(colors::from_component(value)?).is_some() {
                    return None;
                }
            }
            // Positions, sizes and the position/size separator carry no
            // paint information this stage models.
            ComponentValue::Token(Token::Number(_), _)
            | ComponentValue::Token(Token::Percentage(_), _)
            | ComponentValue::Token(Token::Dimension { .. }, _)
            | ComponentValue::Token(Token::Delim('/'), _) => {}
            _ => return None,
        }
    }
    Some(())
}

/// Background keywords this stage skips: repetition, position, size,
/// attachment and origin/clip boxes.
fn known_keyword(name: &str) -> bool {
    matches!(
        name,
        "repeat"
            | "no-repeat"
            | "repeat-x"
            | "repeat-y"
            | "space"
            | "round"
            | "center"
            | "top"
            | "bottom"
            | "left"
            | "right"
            | "fixed"
            | "scroll"
            | "local"
            | "border-box"
            | "padding-box"
            | "content-box"
            | "cover"
            | "contain"
            | "auto"
            | "none"
            | "text"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::appearance::gradients::{GradientKind, GradientStop};

    #[test]
    fn plain_color_background() {
        let background = parse("red").unwrap();
        assert_eq!(background.color, Some(Color::opaque(255, 0, 0)));
        assert!(background.layers.is_empty());
    }

    #[test]
    fn gradient_layer_with_color() {
        let background = parse("linear-gradient(to right, red, blue) no-repeat center").unwrap();
        assert_eq!(background.color, None);
        assert_eq!(background.layers.len(), 1);
        assert_eq!(
            background.layers[0],
            BackgroundImage::Gradient(Gradient {
                kind: GradientKind::Linear,
                angle: Some(90.0),
                center: None,
                stops: vec![
                    GradientStop {
                        color: Color::opaque(255, 0, 0),
                        position: None
                    },
                    GradientStop {
                        color: Color::opaque(0, 0, 255),
                        position: None
                    },
                ],
            })
        );
        // Keyword noise is skipped, not rejected.
        assert!(parse("center / cover no-repeat url(bg.png)").is_some());
    }

    #[test]
    fn url_layer_and_hex_color() {
        let background = parse("url(bg.png) #f00").unwrap();
        assert_eq!(
            background.layers,
            vec![BackgroundImage::Url("bg.png".to_string())]
        );
        assert_eq!(background.color, Some(Color::opaque(255, 0, 0)));
    }

    #[test]
    fn multiple_layers_paint_top_first() {
        let background = parse("url(top.png), radial-gradient(circle, red, blue), green").unwrap();
        assert_eq!(background.layers.len(), 2);
        assert_eq!(
            background.layers[0],
            BackgroundImage::Url("top.png".to_string())
        );
        assert_eq!(background.layers[1].gradient_kind(), GradientKind::Radial);
        assert_eq!(background.color, Some(Color::opaque(0, 128, 0)));
    }

    #[test]
    fn percentages_and_positions_are_tolerated() {
        let background = parse("0 0 / 50% 50% url(x.png)").unwrap();
        assert_eq!(background.layers.len(), 1);
    }

    #[test]
    fn malformed_backgrounds_are_rejected() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("nope"), None);
        assert_eq!(parse("red green"), None);
        assert_eq!(parse("linear-gradient(red)"), None);
        assert_eq!(parse("url(a.png) url(b.png) red red"), None);
    }

    // Small helper keeping the multi-layer assertion readable.
    impl BackgroundImage {
        fn gradient_kind(&self) -> GradientKind {
            match self {
                BackgroundImage::Gradient(gradient) => gradient.kind,
                BackgroundImage::Url(_) => panic!("expected a gradient layer"),
            }
        }
    }
}
