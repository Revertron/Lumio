use std::borrow::Cow;
use quick_xml::events::Event;
use quick_xml::Reader;
use speedy2d::color::Color;

use super::primitives::*;
use super::selector::*;

pub struct DrawableParser;

impl DrawableParser {
    /// Parse a selector XML (Android StateListDrawable style)
    pub fn parse_selector(xml: &str) -> Result<StateSelector, String> {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);

        let mut selector = StateSelector::new();

        loop {
            match reader.read_event() {
                Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                    let tag_name = String::from_utf8(e.name().0.to_vec())
                        .map_err(|e| e.to_string())?;

                    match tag_name.as_str() {
                        "selector" => {
                            // Root element, continue parsing children
                        }
                        "item" => {
                            let matcher = Self::parse_state_matcher(&e)?;
                            let drawable = Self::parse_item_content(&mut reader)?;
                            selector.add_state(matcher, drawable);
                        }
                        _ => {}
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(format!("XML parse error: {}", e)),
                _ => {}
            }
        }

        Ok(selector)
    }

    /// Parse state attributes from <item> element
    fn parse_state_matcher(element: &quick_xml::events::BytesStart) -> Result<StateMatcher, String> {
        let mut matcher = StateMatcher::new();

        for attr in element.attributes() {
            let attr = attr.map_err(|e| e.to_string())?;
            let key = String::from_utf8(attr.key.0.to_vec())
                .map_err(|e| e.to_string())?;
            let value = match attr.value {
                Cow::Borrowed(v) => String::from_utf8(v.to_vec()).map_err(|e| e.to_string())?,
                Cow::Owned(v) => String::from_utf8(v).map_err(|e| e.to_string())?,
            };

            match key.as_str() {
                "state_pressed" => matcher.pressed = Some(value.parse().unwrap_or(false)),
                "state_hovered" => matcher.hovered = Some(value.parse().unwrap_or(false)),
                "state_focused" => matcher.focused = Some(value.parse().unwrap_or(false)),
                "state_checked" => matcher.checked = Some(value.parse().unwrap_or(false)),
                "state_enabled" => matcher.enabled = Some(value.parse().unwrap_or(true)),
                "state_focusable" => matcher.focusable = Some(value.parse().unwrap_or(true)),
                _ => {}
            }
        }

        Ok(matcher)
    }

    /// Parse the content inside an <item> element
    fn parse_item_content(reader: &mut Reader<&[u8]>) -> Result<Drawable, String> {
        loop {
            match reader.read_event() {
                Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                    let tag_name = String::from_utf8(e.name().0.to_vec())
                        .map_err(|e| e.to_string())?;

                    return match tag_name.as_str() {
                        "layer-list" => Self::parse_layer_list(reader),
                        "shape" => Self::parse_shape(reader, &e),
                        _ => Ok(Drawable::default())
                    };
                }
                Ok(Event::End(_)) => {
                    // End of <item>, return empty drawable
                    return Ok(Drawable::default());
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(format!("XML parse error: {}", e)),
                _ => {}
            }
        }

        Ok(Drawable::default())
    }

    /// Parse <layer-list> element
    fn parse_layer_list(reader: &mut Reader<&[u8]>) -> Result<Drawable, String> {
        let mut commands = Vec::new();
        let mut depth = 1; // We're inside layer-list

        loop {
            match reader.read_event() {
                Ok(Event::Start(e)) => {
                    let tag_name = String::from_utf8(e.name().0.to_vec())
                        .map_err(|e| e.to_string())?;

                    if tag_name == "layer-list" {
                        depth += 1;
                    } else if tag_name == "item" {
                        let drawable = Self::parse_item_content(reader)?;
                        commands.extend(drawable.commands);
                    }
                }
                Ok(Event::Empty(e)) => {
                    let tag_name = String::from_utf8(e.name().0.to_vec())
                        .map_err(|e| e.to_string())?;

                    if tag_name == "item" {
                        // Empty item, skip
                    }
                }
                Ok(Event::End(e)) => {
                    let tag_name = String::from_utf8(e.name().0.to_vec())
                        .map_err(|e| e.to_string())?;

                    if tag_name == "layer-list" {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(format!("XML parse error: {}", e)),
                _ => {}
            }
        }

        Ok(Drawable { commands })
    }

    /// Parse <shape> element
    fn parse_shape(
        reader: &mut Reader<&[u8]>,
        start: &quick_xml::events::BytesStart
    ) -> Result<Drawable, String> {
        // Get shape type attribute
        let shape_type = Self::get_attr_opt(start, "type").unwrap_or_else(|| "rect".to_string());

        let mut fill: Option<Paint> = None;
        let mut strokes = Vec::new();
        let mut depth = 1; // We're inside shape

        // Parse child elements
        loop {
            match reader.read_event() {
                Ok(Event::Empty(e)) | Ok(Event::Start(e)) => {
                    let tag_name = String::from_utf8(e.name().0.to_vec())
                        .map_err(|e| e.to_string())?;

                    match tag_name.as_str() {
                        "solid" => {
                            let color = Self::get_attr(&e, "color")?;
                            fill = Some(Paint {
                                kind: Self::parse_paint_kind(&color)?,
                                opacity: 1.0,
                            });
                        }
                        "stroke" => {
                            let stroke_def = Self::parse_stroke_element(&e)?;
                            strokes.push(stroke_def);
                        }
                        _ => {}
                    }
                }
                Ok(Event::End(e)) => {
                    let tag_name = String::from_utf8(e.name().0.to_vec())
                        .map_err(|e| e.to_string())?;

                    if tag_name == "shape" {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(format!("XML parse error: {}", e)),
                _ => {}
            }
        }

        // Create shape commands
        Self::create_shape_drawable(&shape_type, fill, strokes)
    }

    /// Create drawable from shape definition
    fn create_shape_drawable(
        shape_type: &str,
        fill: Option<Paint>,
        strokes: Vec<StrokeDefinition>
    ) -> Result<Drawable, String> {
        let mut commands = Vec::new();

        match shape_type {
            "rect" => {
                // Add fill rect if needed
                if fill.is_some() {
                    commands.push(DrawCommand::Rect {
                        x: Expr::BoundsLeft,
                        y: Expr::BoundsTop,
                        width: Expr::BoundsWidth,
                        height: Expr::BoundsHeight,
                        fill: fill.clone(),
                        stroke: None,
                    });
                }

                // Add stroke commands
                for stroke_def in strokes {
                    let stroke_commands = Self::create_stroke_commands(&stroke_def)?;
                    commands.extend(stroke_commands);
                }
            }
            _ => {
                return Err(format!("Unsupported shape type: {}", shape_type));
            }
        }

        Ok(Drawable { commands })
    }

    /// Parse stroke element attributes
    fn parse_stroke_element(element: &quick_xml::events::BytesStart) -> Result<StrokeDefinition, String> {
        let width = Self::get_attr(element, "width")?
            .parse::<f32>()
            .map_err(|e| format!("Invalid width: {}", e))?;

        let color = Self::parse_paint_kind(&Self::get_attr(element, "color")?)?;

        let top = Self::get_attr_opt(element, "top")
            .and_then(|s| s.parse::<f32>().ok());

        let bottom = Self::get_attr_opt(element, "bottom")
            .and_then(|s| s.parse::<f32>().ok());

        let left = Self::get_attr_opt(element, "left")
            .and_then(|s| s.parse::<f32>().ok());

        let right = Self::get_attr_opt(element, "right")
            .and_then(|s| s.parse::<f32>().ok());

        let dash_array = Self::get_attr_opt(element, "dash_array")
            .and_then(|s| Self::parse_dash_array(&s));

        Ok(StrokeDefinition {
            width,
            color,
            top,
            bottom,
            left,
            right,
            dash_array,
        })
    }

    /// Create line commands from stroke definition
    fn create_stroke_commands(stroke: &StrokeDefinition) -> Result<Vec<DrawCommand>, String> {
        let width = Expr::Literal(stroke.width);

        let paint = Paint {
            kind: stroke.color.clone(),
            opacity: 1.0,
        };

        let stroke_style = Stroke {
            paint,
            width,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            dash_array: stroke.dash_array.clone(),
        };

        // Determine line type based on which coordinates are specified
        // If top is specified, it's a horizontal line at the top
        // If bottom is specified (and not top), it's a horizontal line at the bottom
        // If left is specified (and no top/bottom), it's a vertical line at the left
        // If right is specified (and no top/bottom/left), it's a vertical line at the right

        if stroke.top.is_some() && stroke.bottom.is_none() {
            // Horizontal line at top edge
            let y_offset = Expr::Literal(stroke.top.unwrap());
            let left_offset = Expr::Literal(stroke.left.unwrap_or(0.0));
            let right_offset = Expr::Literal(stroke.right.unwrap_or(0.0));

            Ok(vec![DrawCommand::Line {
                x1: Expr::Add(Box::new(Expr::BoundsLeft), Box::new(left_offset)),
                y1: Expr::Add(Box::new(Expr::BoundsTop), Box::new(y_offset.clone())),
                x2: Expr::Sub(Box::new(Expr::BoundsRight), Box::new(right_offset)),
                y2: Expr::Add(Box::new(Expr::BoundsTop), Box::new(y_offset)),
                stroke: Some(stroke_style),
            }])
        } else if stroke.bottom.is_some() && stroke.top.is_none() {
            // Horizontal line at bottom edge
            let y_offset = Expr::Literal(stroke.bottom.unwrap());
            let left_offset = Expr::Literal(stroke.left.unwrap_or(0.0));
            let right_offset = Expr::Literal(stroke.right.unwrap_or(0.0));

            Ok(vec![DrawCommand::Line {
                x1: Expr::Add(Box::new(Expr::BoundsLeft), Box::new(left_offset)),
                y1: Expr::Sub(Box::new(Expr::BoundsBottom), Box::new(y_offset.clone())),
                x2: Expr::Sub(Box::new(Expr::BoundsRight), Box::new(right_offset)),
                y2: Expr::Sub(Box::new(Expr::BoundsBottom), Box::new(y_offset)),
                stroke: Some(stroke_style),
            }])
        } else if stroke.left.is_some() && stroke.right.is_none() {
            // Vertical line at left edge
            let x_offset = Expr::Literal(stroke.left.unwrap());
            let top_offset = Expr::Literal(stroke.top.unwrap_or(0.0));
            let bottom_offset = Expr::Literal(stroke.bottom.unwrap_or(0.0));

            Ok(vec![DrawCommand::Line {
                x1: Expr::Add(Box::new(Expr::BoundsLeft), Box::new(x_offset.clone())),
                y1: Expr::Add(Box::new(Expr::BoundsTop), Box::new(top_offset)),
                x2: Expr::Add(Box::new(Expr::BoundsLeft), Box::new(x_offset)),
                y2: Expr::Sub(Box::new(Expr::BoundsBottom), Box::new(bottom_offset)),
                stroke: Some(stroke_style),
            }])
        } else if stroke.right.is_some() && stroke.left.is_none() {
            // Vertical line at right edge
            let x_offset = Expr::Literal(stroke.right.unwrap());
            let top_offset = Expr::Literal(stroke.top.unwrap_or(0.0));
            let bottom_offset = Expr::Literal(stroke.bottom.unwrap_or(0.0));

            Ok(vec![DrawCommand::Line {
                x1: Expr::Sub(Box::new(Expr::BoundsRight), Box::new(x_offset.clone())),
                y1: Expr::Add(Box::new(Expr::BoundsTop), Box::new(top_offset)),
                x2: Expr::Sub(Box::new(Expr::BoundsRight), Box::new(x_offset)),
                y2: Expr::Sub(Box::new(Expr::BoundsBottom), Box::new(bottom_offset)),
                stroke: Some(stroke_style),
            }])
        } else {
            Err("Stroke must specify exactly one primary edge (top OR bottom OR left OR right)".to_string())
        }
    }

    // Helper methods

    fn get_attr(element: &quick_xml::events::BytesStart, name: &str) -> Result<String, String> {
        for attr in element.attributes() {
            let attr = attr.map_err(|e| e.to_string())?;
            let key = String::from_utf8(attr.key.0.to_vec())
                .map_err(|e| e.to_string())?;

            if key == name {
                return match attr.value {
                    Cow::Borrowed(v) => String::from_utf8(v.to_vec()).map_err(|e| e.to_string()),
                    Cow::Owned(v) => String::from_utf8(v).map_err(|e| e.to_string()),
                };
            }
        }

        Err(format!("Missing attribute: {}", name))
    }

    fn get_attr_opt(element: &quick_xml::events::BytesStart, name: &str) -> Option<String> {
        Self::get_attr(element, name).ok()
    }

    /// Parse a color attribute: `@name` becomes a palette token resolved at
    /// draw time, anything else must be a literal `#RRGGBB`/`#AARRGGBB` color.
    fn parse_paint_kind(color_str: &str) -> Result<PaintKind, String> {
        if let Some(token) = color_str.strip_prefix('@') {
            if token.is_empty() {
                return Err("Empty color token name".to_string());
            }
            Ok(PaintKind::Token(token.to_string()))
        } else {
            Ok(PaintKind::Color(Self::parse_color(color_str)?))
        }
    }

    fn parse_color(color_str: &str) -> Result<Color, String> {
        if color_str.starts_with('#') {
            let hex = &color_str[1..];
            match hex.len() {
                6 => {
                    // #RRGGBB -> add full alpha
                    let rgb = u32::from_str_radix(hex, 16)
                        .map_err(|e| format!("Invalid color: {}", e))?;
                    Ok(Color::from_hex_rgb(0xff000000 | rgb))
                }
                8 => {
                    // #AARRGGBB
                    let argb = u32::from_str_radix(hex, 16)
                        .map_err(|e| format!("Invalid color: {}", e))?;
                    Ok(Color::from_hex_argb(argb))
                }
                _ => Err("Invalid color format (use #RRGGBB or #AARRGGBB)".to_string())
            }
        } else {
            Err("Color must start with #".to_string())
        }
    }

    fn parse_dash_array(dash_str: &str) -> Option<Vec<f32>> {
        let parts: Vec<&str> = dash_str.split(',').map(|s| s.trim()).collect();
        let mut result = Vec::new();

        for part in parts {
            if let Ok(val) = part.parse::<f32>() {
                result.push(val);
            } else {
                return None;
            }
        }

        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::themes::ViewState;

    #[test]
    fn test_solid_color_token_parses() {
        let xml = r#"<selector>
            <item>
                <layer-list>
                    <item>
                        <shape type="rect">
                            <solid color="@surface"/>
                            <stroke width="1" color="@border_dark" top="0"/>
                        </shape>
                    </item>
                </layer-list>
            </item>
        </selector>"#;

        let selector = DrawableParser::parse_selector(xml).expect("selector should parse");
        let drawable = selector.get_drawable(&ViewState::default()).expect("default state");

        let mut tokens = Vec::new();
        fn collect(commands: &[DrawCommand], tokens: &mut Vec<String>) {
            for cmd in commands {
                match cmd {
                    DrawCommand::Rect { fill, stroke, .. } => {
                        if let Some(Paint { kind: PaintKind::Token(t), .. }) = fill {
                            tokens.push(t.clone());
                        }
                        if let Some(s) = stroke {
                            if let PaintKind::Token(t) = &s.paint.kind {
                                tokens.push(t.clone());
                            }
                        }
                    }
                    DrawCommand::Line { stroke: Some(s), .. } => {
                        if let PaintKind::Token(t) = &s.paint.kind {
                            tokens.push(t.clone());
                        }
                    }
                    DrawCommand::Group { commands } => collect(commands, tokens),
                    _ => {}
                }
            }
        }
        collect(&drawable.commands, &mut tokens);

        assert!(tokens.contains(&"surface".to_string()));
        assert!(tokens.contains(&"border_dark".to_string()));
    }

    #[test]
    fn test_invalid_color_still_errors() {
        assert!(DrawableParser::parse_paint_kind("red").is_err());
        assert!(DrawableParser::parse_paint_kind("@").is_err());
        assert!(matches!(DrawableParser::parse_paint_kind("@text"), Ok(PaintKind::Token(t)) if t == "text"));
        assert!(matches!(DrawableParser::parse_paint_kind("#ffffff"), Ok(PaintKind::Color(_))));
    }
}

/// Stroke definition for parsing
#[derive(Debug, Clone)]
struct StrokeDefinition {
    width: f32,
    color: PaintKind,
    top: Option<f32>,
    bottom: Option<f32>,
    left: Option<f32>,
    right: Option<f32>,
    dash_array: Option<Vec<f32>>,
}
