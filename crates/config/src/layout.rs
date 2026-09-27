//! The layout tree: which component fills each slot and how slots are arranged.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// How a container lays out its children.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Row,
    Column,
}

/// One `[slot.<name>]` table as written in `layout.toml`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SlotSpec {
    pub component: Option<String>,
    pub direction: Option<Direction>,
    pub children: Option<Vec<String>>,
    /// A size token name, such as `size.sidebar-width`.
    pub size: Option<String>,
    pub visible: Option<bool>,
}

/// The whole `layout.toml` file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LayoutSpec {
    pub root: Option<String>,
    pub slot: BTreeMap<String, SlotSpec>,
}

/// What fills a slot in the built tree.
#[derive(Clone, Debug, PartialEq)]
pub enum SlotContent {
    Component(String),
    Container {
        direction: Direction,
        children: Vec<LayoutNode>,
    },
}

/// A slot in the built tree.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutNode {
    pub slot: String,
    pub content: SlotContent,
    pub size: Option<String>,
    pub visible: bool,
}

impl LayoutNode {
    /// Finds a slot anywhere under this node.
    pub fn find(&self, slot: &str) -> Option<&LayoutNode> {
        if self.slot == slot {
            return Some(self);
        }
        self.children().iter().find_map(|child| child.find(slot))
    }

    pub fn children(&self) -> &[LayoutNode] {
        match &self.content {
            SlotContent::Container { children, .. } => children,
            SlotContent::Component(_) => &[],
        }
    }

    /// Every slot name in depth-first order.
    pub fn slot_names(&self) -> Vec<&str> {
        let mut names = vec![self.slot.as_str()];
        for child in self.children() {
            names.extend(child.slot_names());
        }
        names
    }

    /// Every component name in depth-first order.
    pub fn components(&self) -> Vec<&str> {
        match &self.content {
            SlotContent::Component(name) => vec![name.as_str()],
            SlotContent::Container { children, .. } => {
                children.iter().flat_map(LayoutNode::components).collect()
            }
        }
    }
}

/// Why a layout couldn't be built. `slot` names the slot at fault.
#[derive(Clone, Debug, PartialEq)]
pub enum LayoutError {
    MissingRoot,
    UnknownSlot { slot: String, parent: String },
    BothComponentAndChildren { slot: String },
    Empty { slot: String },
    Cycle { slot: String },
    UsedTwice { slot: String },
}

impl LayoutError {
    pub fn slot(&self) -> Option<&str> {
        match self {
            LayoutError::MissingRoot => None,
            LayoutError::UnknownSlot { slot, .. }
            | LayoutError::BothComponentAndChildren { slot }
            | LayoutError::Empty { slot }
            | LayoutError::Cycle { slot }
            | LayoutError::UsedTwice { slot } => Some(slot),
        }
    }
}

impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LayoutError::MissingRoot => f.write_str("the layout needs a `root` slot"),
            LayoutError::UnknownSlot { slot, parent } => {
                write!(f, "slot `{parent}` names unknown slot `{slot}`")
            }
            LayoutError::BothComponentAndChildren { slot } => {
                write!(f, "slot `{slot}` has both a component and children")
            }
            LayoutError::Empty { slot } => {
                write!(f, "slot `{slot}` needs a component or children")
            }
            LayoutError::Cycle { slot } => write!(f, "slot `{slot}` contains itself"),
            LayoutError::UsedTwice { slot } => {
                write!(f, "slot `{slot}` appears in more than one place")
            }
        }
    }
}

impl LayoutSpec {
    /// Layers `overlay` on top: the root and each named slot's fields replace ours.
    pub fn layer(&mut self, overlay: LayoutSpec) {
        if overlay.root.is_some() {
            self.root = overlay.root;
        }
        for (name, slot) in overlay.slot {
            let existing = self.slot.entry(name).or_default();
            existing.layer(slot);
        }
    }

    /// Builds the tree from the root slot.
    pub fn build(&self) -> Result<LayoutNode, LayoutError> {
        let root = self.root.as_deref().ok_or(LayoutError::MissingRoot)?;
        if !self.slot.contains_key(root) {
            return Err(LayoutError::UnknownSlot {
                slot: root.to_string(),
                parent: "root".to_string(),
            });
        }
        let mut builder = Builder {
            spec: self,
            path: Vec::new(),
            used: Vec::new(),
        };
        builder.build(root)
    }
}

impl SlotSpec {
    fn layer(&mut self, overlay: SlotSpec) {
        if overlay.component.is_some() {
            self.children = None;
            self.direction = None;
            self.component = overlay.component;
        }
        if overlay.children.is_some() {
            self.component = None;
            self.children = overlay.children;
        }
        self.direction = overlay.direction.or(self.direction);
        self.size = overlay.size.or(self.size.take());
        self.visible = overlay.visible.or(self.visible);
    }
}

struct Builder<'a> {
    spec: &'a LayoutSpec,
    path: Vec<String>,
    used: Vec<String>,
}

impl Builder<'_> {
    fn build(&mut self, name: &str) -> Result<LayoutNode, LayoutError> {
        self.mark(name)?;
        let slot = &self.spec.slot[name];
        let content = self.content(name, slot)?;
        self.path.pop();
        Ok(LayoutNode {
            slot: name.to_string(),
            content,
            size: slot.size.clone(),
            visible: slot.visible.unwrap_or(true),
        })
    }

    fn mark(&mut self, name: &str) -> Result<(), LayoutError> {
        let slot = name.to_string();
        if self.path.contains(&slot) {
            return Err(LayoutError::Cycle { slot });
        }
        if self.used.contains(&slot) {
            return Err(LayoutError::UsedTwice { slot });
        }
        self.path.push(slot.clone());
        self.used.push(slot);
        Ok(())
    }

    fn content(&mut self, name: &str, slot: &SlotSpec) -> Result<SlotContent, LayoutError> {
        let slot_name = || name.to_string();
        match (&slot.component, &slot.children) {
            (Some(_), Some(_)) => Err(LayoutError::BothComponentAndChildren { slot: slot_name() }),
            (Some(component), None) => Ok(SlotContent::Component(component.clone())),
            (None, Some(children)) => self.container(name, slot, children),
            (None, None) => Err(LayoutError::Empty { slot: slot_name() }),
        }
    }

    fn container(
        &mut self,
        name: &str,
        slot: &SlotSpec,
        children: &[String],
    ) -> Result<SlotContent, LayoutError> {
        let mut built = Vec::with_capacity(children.len());
        for child in children {
            if !self.spec.slot.contains_key(child) {
                return Err(LayoutError::UnknownSlot {
                    slot: child.clone(),
                    parent: name.to_string(),
                });
            }
            built.push(self.build(child)?);
        }
        Ok(SlotContent::Container {
            direction: slot.direction.unwrap_or(Direction::Column),
            children: built,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(text: &str) -> LayoutSpec {
        toml::from_str(text).unwrap()
    }

    #[test]
    fn builds_nested_slots() {
        let layout = spec(
            "root = \"w\"\n[slot.w]\ndirection = \"row\"\nchildren = [\"a\", \"b\"]\n\
             [slot.a]\ncomponent = \"file-tree\"\n[slot.b]\ncomponent = \"editor\"\n",
        )
        .build()
        .unwrap();
        assert_eq!(layout.components(), ["file-tree", "editor"]);
        assert_eq!(
            layout.find("b").unwrap().content,
            SlotContent::Component("editor".into())
        );
    }

    #[test]
    fn rejects_cycles() {
        let error =
            spec("root = \"a\"\n[slot.a]\nchildren = [\"b\"]\n[slot.b]\nchildren = [\"a\"]\n")
                .build()
                .unwrap_err();
        assert_eq!(error, LayoutError::Cycle { slot: "a".into() });
    }

    #[test]
    fn rejects_unknown_children() {
        let error = spec("root = \"a\"\n[slot.a]\nchildren = [\"ribbon\"]\n")
            .build()
            .unwrap_err();
        assert_eq!(error.slot(), Some("ribbon"));
    }

    #[test]
    fn rejects_a_slot_used_twice() {
        let error = spec(
            "root = \"a\"\n[slot.a]\nchildren = [\"b\", \"b\"]\n[slot.b]\ncomponent = \"x\"\n",
        )
        .build()
        .unwrap_err();
        assert_eq!(error, LayoutError::UsedTwice { slot: "b".into() });
    }

    #[test]
    fn overlay_can_swap_a_component() {
        let mut base = spec("root = \"a\"\n[slot.a]\ncomponent = \"outline\"\nvisible = false\n");
        base.layer(spec("[slot.a]\ncomponent = \"backlinks\"\n"));
        let tree = base.build().unwrap();
        assert_eq!(tree.content, SlotContent::Component("backlinks".into()));
        assert!(!tree.visible);
    }

    #[test]
    fn overlay_can_turn_a_component_into_a_container() {
        let mut base =
            spec("root = \"a\"\n[slot.a]\ncomponent = \"editor\"\n[slot.b]\ncomponent = \"x\"\n");
        base.layer(spec("[slot.a]\nchildren = [\"b\"]\n"));
        assert_eq!(base.build().unwrap().components(), ["x"]);
    }
}
