pub mod label;
pub mod button;
pub mod edit;
pub mod checkbox;
pub mod list;
pub mod recyclerview;
pub mod imagebutton;
pub mod imageview;
pub mod popupmenu;
pub mod radiobutton;
pub mod combobox;
pub mod scrollview;
pub mod progressbar;
pub mod tabview;
pub mod separator;
pub mod splitpanel;
pub mod statusbar;
pub mod memo;
pub mod notification_stack;
pub mod tableview;
pub mod grid;
pub mod richtext;
pub mod menubar;
pub mod slider;
pub mod treeview;
pub mod iconlist;

use super::themes::{Typeface, ViewState};
use super::traits::WeakElement;
use super::types::Rect;
use super::text::TextBlock;
use std::collections::HashMap;
use std::str::FromStr;
use super::common::random_string;
use super::events::{EventCallback, EventType};
use super::styles::selector::{MainSelector, FontSelector};
use super::view_base::FontManager;
pub use self::label::Label;
pub use self::button::Button;
pub use self::edit::Edit;
pub use self::checkbox::CheckBox;
pub use self::list::List;
pub use self::recyclerview::{RecyclerView, RecyclerAdapter, ViewHolder, LayoutManager, LinearLayoutManager};
pub use self::imagebutton::ImageButton;
pub use self::imageview::ImageView;
pub use self::popupmenu::{PopupMenu, MenuItem};
pub use self::radiobutton::RadioButton;
pub use self::combobox::ComboBox;
pub use self::scrollview::ScrollView;
pub use self::progressbar::ProgressBar;
pub use self::tabview::TabView;
pub use self::separator::Separator;
pub use self::splitpanel::SplitPanel;
pub use self::statusbar::StatusBar;
pub use self::memo::Memo;
pub use self::notification_stack::NotificationStack;
pub use self::tableview::{TableView, TableColumn, TableRow, ColumnDef, ColumnWidth, SortDirection};
pub use self::grid::Grid;
pub use self::richtext::{RichText, SpanStyle};
pub use self::menubar::{MenuBar, Menu, MenuItemTag, MenuData};
pub use self::slider::{Slider, LabelStyle};
pub use self::treeview::{TreeView, TreeNode};
pub use self::iconlist::{IconList, IconListItem};

pub const BUTTON_MIN_WIDTH: i32 = 80;
pub const BUTTON_MIN_HEIGHT: i32 = 24;

/// Stores all main fields of elements.
pub struct FieldsMain {
    pub width: Dimension,
    pub height: Dimension,
    pub rect: Rect<i32>,
    pub padding: Borders,
    pub margin: Borders,
    pub scale: f64,
    pub id: String,
    pub state: ViewState,
    pub break_line: bool,
    pub visibility: Visibility,
    pub background: Option<MainSelector>,
    pub foreground: Option<MainSelector>,
    pub border_color: Option<u32>,
    pub parent: Option<WeakElement>,
    pub font_manager: FontManager,
    pub tooltip: Option<String>,
    /// Explicit accessible name for screen readers (Android's
    /// `contentDescription`); overrides the widget-derived label.
    pub content_description: Option<String>,
    /// Id of another view (usually a `Label`) whose text names this view for
    /// screen readers, like a `<label for=..>` association.
    pub labelled_by: Option<String>,
    pub gravity: Gravity,
    pub layout_params: LayoutParams,
    pub listeners: HashMap<EventType, EventCallback>
}

impl FieldsMain {
    /// Convenient method to create main fields.
    /// Most of these values will be changed in `layout()` methods.
    pub fn with_rect(rect: Rect<i32>, width: Dimension, height: Dimension) -> Self {
        FieldsMain {
            width,
            height,
            rect,
            padding: Borders::default(),
            margin: Borders::default(),
            scale: 1.0,
            id: random_string(16),
            state: ViewState::default(),
            break_line: false,
            visibility: Visibility::Visible,
            background: None,
            foreground: None,
            border_color: None,
            parent: None,
            font_manager: FontManager::new(),
            tooltip: None,
            content_description: None,
            labelled_by: None,
            gravity: Gravity::default(),
            layout_params: LayoutParams::default(),
            listeners: HashMap::new()
        }
    }

    /// Get the effective typeface (for backward compatibility)
    pub fn get_typeface(&self, parent_typeface: &Typeface) -> Typeface {
        self.font_manager.get_typeface(parent_typeface)
    }

    /// Set the typeface (for backward compatibility)
    pub fn set_typeface(&mut self, typeface: Option<Typeface>) {
        self.font_manager.set(typeface);
    }

    /// Get the stored typeface (for backward compatibility)
    pub fn typeface(&self) -> Option<Typeface> {
        self.font_manager.get()
    }
}

/// Stores main fields (properties) of elements, plus fields for text.
pub struct FieldsTexted {
    pub main: FieldsMain,
    pub text: String,
    pub text_size: f32,
    pub line_height: f32,
    pub single_line: bool,
    pub cached_text: Option<TextBlock>,
    pub font: FontSelector
}

/// Represents padding (inner spaces) or margin (outer spaces) of any element.
#[derive(Clone, Copy, Debug)]
pub struct Borders {
    pub top: i32,
    pub left: i32,
    pub right: i32,
    pub bottom: i32
}

#[allow(unused)]
impl Borders {
    pub fn new(top: i32, left: i32, right: i32, bottom: i32) -> Self {
        Self { top, left, right, bottom }
    }

    pub fn with_padding(padding: i32) -> Self {
        Self { top: padding, left: padding, right: padding, bottom: padding }
    }

    pub fn set_all(&mut self, padding: i32) {
        self.top = padding;
        self.left = padding;
        self.right = padding;
        self.bottom = padding;
    }

    pub fn scaled(&self, scale: f64) -> Self {
        Self {
            top: (self.top as f64 * scale).ceil() as i32,
            left: (self.left as f64 * scale).ceil() as i32,
            right: (self.right as f64 * scale).ceil() as i32,
            bottom: (self.bottom as f64 * scale).ceil() as i32
        }
    }
}

impl Default for Borders {
    fn default() -> Self {
        Self::with_padding(0)
    }
}

/// Elements width or height. They can fill up all space in some direction (Max),
/// or just enough space to wrap its content (Min), or set concrete size
/// in terms of device independent pixels (Dip, they will be scaled),
/// or some fraction of available area (Percent).
#[derive(Clone, Copy, Debug)]
pub enum Dimension {
    Min,
    Max,
    Dip(u32),
    Percent(f32)
}

impl FromStr for Dimension {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let result = match s {
            "max" => Dimension::Max,
            "min" => Dimension::Min,
            &_ => {
                if s.ends_with("%") {
                    let float = match s[0..s.len()-1].parse::<f32>() {
                        Ok(float) => float,
                        Err(e) => {
                            println!("Error parsing {}, {}", s, e);
                            0f32
                        }
                    };
                    Dimension::Percent(float)
                } else {
                    let int = match s[0..s.len()].parse::<u32>() {
                        Ok(int) => int,
                        Err(e) => {
                            println!("Error parsing {}, {}", s, e);
                            0u32
                        }
                    };
                    Dimension::Dip(int)
                }
            }
        };
        Ok(result)
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Visibility {
    Visible,
    Hidden,
    Gone,
}

impl Default for Visibility {
    fn default() -> Self {
        Visibility::Visible
    }
}

impl FromStr for Visibility {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let result = match s {
            "hidden" => Visibility::Hidden,
            "gone" => Visibility::Gone,
            _ => Visibility::Visible,
        };
        Ok(result)
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Direction {
    Horizontal,
    Vertical
}

impl Default for Direction {
    fn default() -> Self {
        Direction::Horizontal
    }
}

impl FromStr for Direction {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let result = match s {
            "vertical" => Direction::Vertical,
            &_ => Direction::Horizontal
        };
        Ok(result)
    }
}

/// Hint for a parent container telling it where this view should sit
/// inside the space allocated to it. Components combine with `|`,
/// e.g. `Gravity::RIGHT | Gravity::CENTER_VERTICAL`. In a linear `Frame`
/// only the cross-axis component takes effect (horizontal in a vertical
/// frame, vertical in a horizontal frame).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Gravity(u8);

impl Gravity {
    /// Align to the parent's left edge.
    pub const LEFT: Gravity              = Gravity(0b0000_0001);
    /// Align to the parent's right edge.
    pub const RIGHT: Gravity             = Gravity(0b0000_0010);
    /// Center horizontally within the parent.
    pub const CENTER_HORIZONTAL: Gravity = Gravity(0b0000_0100);
    /// Align to the parent's top edge.
    pub const TOP: Gravity               = Gravity(0b0000_1000);
    /// Align to the parent's bottom edge.
    pub const BOTTOM: Gravity            = Gravity(0b0001_0000);
    /// Center vertically within the parent.
    pub const CENTER_VERTICAL: Gravity   = Gravity(0b0010_0000);

    /// Top-left corner (`TOP | LEFT`); the default gravity.
    pub const TOP_LEFT: Gravity     = Gravity(Self::TOP.0 | Self::LEFT.0);
    /// Top-right corner (`TOP | RIGHT`).
    pub const TOP_RIGHT: Gravity    = Gravity(Self::TOP.0 | Self::RIGHT.0);
    /// Bottom-left corner (`BOTTOM | LEFT`).
    pub const BOTTOM_LEFT: Gravity  = Gravity(Self::BOTTOM.0 | Self::LEFT.0);
    /// Bottom-right corner (`BOTTOM | RIGHT`).
    pub const BOTTOM_RIGHT: Gravity = Gravity(Self::BOTTOM.0 | Self::RIGHT.0);
    /// Centered on both axes (`CENTER_HORIZONTAL | CENTER_VERTICAL`).
    pub const CENTER: Gravity       = Gravity(Self::CENTER_HORIZONTAL.0 | Self::CENTER_VERTICAL.0);

    /// The horizontal component as an `HAlign` (`Left` when no horizontal
    /// bit is set).
    pub fn horizontal(self) -> HAlign {
        if self.0 & Self::CENTER_HORIZONTAL.0 != 0 {
            HAlign::Center
        } else if self.0 & Self::RIGHT.0 != 0 {
            HAlign::Right
        } else {
            HAlign::Left
        }
    }

    /// The vertical component as a `VAlign` (`Top` when no vertical bit is
    /// set).
    pub fn vertical(self) -> VAlign {
        if self.0 & Self::CENTER_VERTICAL.0 != 0 {
            VAlign::Center
        } else if self.0 & Self::BOTTOM.0 != 0 {
            VAlign::Bottom
        } else {
            VAlign::Top
        }
    }
}

impl Default for Gravity {
    fn default() -> Self {
        Gravity::TOP_LEFT
    }
}

impl std::ops::BitOr for Gravity {
    type Output = Gravity;
    fn bitor(self, rhs: Self) -> Self::Output {
        Gravity(self.0 | rhs.0)
    }
}

impl FromStr for Gravity {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut bits: u8 = 0;
        let mut horizontal_set = false;
        let mut vertical_set = false;
        for token in s.split('|') {
            match token.trim() {
                "left" => { bits |= Gravity::LEFT.0; horizontal_set = true; }
                "right" => { bits |= Gravity::RIGHT.0; horizontal_set = true; }
                "center_horizontal" => { bits |= Gravity::CENTER_HORIZONTAL.0; horizontal_set = true; }
                "top" => { bits |= Gravity::TOP.0; vertical_set = true; }
                "bottom" => { bits |= Gravity::BOTTOM.0; vertical_set = true; }
                "center_vertical" => { bits |= Gravity::CENTER_VERTICAL.0; vertical_set = true; }
                "center" => {
                    bits |= Gravity::CENTER.0;
                    horizontal_set = true;
                    vertical_set = true;
                }
                _ => {}
            }
        }
        if !horizontal_set { bits |= Gravity::LEFT.0; }
        if !vertical_set { bits |= Gravity::TOP.0; }
        Ok(Gravity(bits))
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HAlign { Left, Center, Right }

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum VAlign { Top, Center, Bottom }

/// Which edge a child takes inside a `DockLayout` parent.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Dock {
    Left,
    Top,
    Right,
    Bottom,
    /// Take all space left over by the docked siblings (typically the last child).
    #[default]
    Fill
}

impl FromStr for Dock {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "left" => Dock::Left,
            "top" => Dock::Top,
            "right" => Dock::Right,
            "bottom" => Dock::Bottom,
            _ => Dock::Fill
        })
    }
}

/// Per-child hints consumed by the parent's `Layout`, not by the view itself:
/// `dock` (XML attr `dock`) is read by `DockLayout`, `weight` (XML attr
/// `weight`) is read by `LinearLayout` to share leftover space between `Max`
/// children proportionally.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct LayoutParams {
    pub dock: Dock,
    pub weight: f32
}

impl Default for LayoutParams {
    fn default() -> Self {
        Self { dock: Dock::default(), weight: 1.0 }
    }
}