//! The tree of split panes: which panes sit side by side or stacked, how
//! the space is shared, and which pane lies in a direction from another.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{Bounds, Pixels};

/// The smallest share of a split either side can shrink to.
pub const MIN_RATIO: f32 = 0.1;

/// How a split arranges its two sides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Side by side, divided by a vertical line.
    Row,
    /// Stacked, divided by a horizontal line.
    Column,
}

/// A direction to move focus in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
}

/// Identifies a split, for dragging its divider.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SplitId(pub usize);

/// A pane's place in the unit square the tree fills.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    const UNIT: Rect = Rect {
        x: 0.,
        y: 0.,
        width: 1.,
        height: 1.,
    };

    fn right(&self) -> f32 {
        self.x + self.width
    }

    fn bottom(&self) -> f32 {
        self.y + self.height
    }

    fn vertical_overlap(&self, other: &Rect) -> f32 {
        (self.bottom().min(other.bottom()) - self.y.max(other.y)).max(0.)
    }

    fn divide(&self, axis: Axis, ratio: f32) -> (Rect, Rect) {
        match axis {
            Axis::Row => {
                let first = self.width * ratio;
                (
                    Rect {
                        width: first,
                        ..*self
                    },
                    Rect {
                        x: self.x + first,
                        width: self.width - first,
                        ..*self
                    },
                )
            }
            Axis::Column => {
                let first = self.height * ratio;
                (
                    Rect {
                        height: first,
                        ..*self
                    },
                    Rect {
                        y: self.y + first,
                        height: self.height - first,
                        ..*self
                    },
                )
            }
        }
    }
}

/// Two sides sharing space.
#[derive(Clone, Debug)]
pub struct Split<T> {
    pub id: SplitId,
    pub axis: Axis,
    /// The first side's share, between [`MIN_RATIO`] and `1 - MIN_RATIO`.
    pub ratio: f32,
    pub first: Box<Node<T>>,
    pub second: Box<Node<T>>,
    /// Where the split was last drawn, for turning a drag into a ratio.
    pub bounds: Rc<Cell<Bounds<Pixels>>>,
}

/// A pane, or a split of two subtrees.
#[derive(Clone, Debug)]
pub enum Node<T> {
    Leaf(T),
    Split(Split<T>),
}

/// The whole tree. It always holds at least one pane.
#[derive(Clone, Debug)]
pub struct PaneTree<T> {
    root: Node<T>,
    next_split: usize,
}

impl<T: Clone + PartialEq> PaneTree<T> {
    pub fn new(pane: T) -> Self {
        PaneTree {
            root: Node::Leaf(pane),
            next_split: 0,
        }
    }

    pub fn root(&self) -> &Node<T> {
        &self.root
    }

    /// Every pane, left to right and top to bottom.
    pub fn panes(&self) -> Vec<T> {
        let mut panes = Vec::new();
        collect_leaves(&self.root, &mut panes);
        panes
    }

    pub fn len(&self) -> usize {
        self.panes().len()
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn contains(&self, pane: &T) -> bool {
        self.panes().contains(pane)
    }

    /// Puts `new` beside `target`, after it along `axis`.
    pub fn split(&mut self, target: &T, new: T, axis: Axis) -> bool {
        let id = SplitId(self.next_split);
        let Some(node) = find_leaf_mut(&mut self.root, target) else {
            return false;
        };
        self.next_split += 1;
        let old = std::mem::replace(node, Node::Leaf(new.clone()));
        *node = Node::Split(Split {
            id,
            axis,
            ratio: 0.5,
            first: Box::new(old),
            second: Box::new(Node::Leaf(new)),
            bounds: Rc::default(),
        });
        true
    }

    /// Removes `target`, giving its space to its sibling. The last pane
    /// can't be removed.
    pub fn remove(&mut self, target: &T) -> bool {
        let root = std::mem::replace(&mut self.root, Node::Leaf(target.clone()));
        let (root, removed) = remove_from(root, target);
        self.root = root;
        removed
    }

    /// Sets a split's ratio, clamped so neither side vanishes.
    pub fn set_ratio(&mut self, id: SplitId, ratio: f32) {
        if let Some(split) = find_split_mut(&mut self.root, id) {
            split.ratio = ratio.clamp(MIN_RATIO, 1. - MIN_RATIO);
        }
    }

    pub fn split_by_id(&self, id: SplitId) -> Option<&Split<T>> {
        find_split(&self.root, id)
    }

    /// Each pane with its place in the unit square.
    pub fn rects(&self) -> Vec<(T, Rect)> {
        let mut rects = Vec::new();
        collect_rects(&self.root, Rect::UNIT, &mut rects);
        rects
    }

    /// The pane to move focus to from `from`: the nearest pane on that
    /// side that shares some height with it, or else the pane before or
    /// after it in reading order, which reaches panes stacked above and
    /// below.
    pub fn neighbor(&self, from: &T, direction: Direction) -> Option<T> {
        let rects = self.rects();
        let (_, current) = rects.iter().find(|(pane, _)| pane == from)?;
        let beside = rects
            .iter()
            .filter(|(pane, rect)| pane != from && is_beside(current, rect, direction))
            .min_by(|a, b| {
                side_distance(current, &a.1, direction)
                    .total_cmp(&side_distance(current, &b.1, direction))
                    .then(
                        b.1.vertical_overlap(current)
                            .total_cmp(&a.1.vertical_overlap(current)),
                    )
            })
            .map(|(pane, _)| pane.clone());
        beside.or_else(|| self.adjacent_in_order(from, direction))
    }

    /// The pane before `pane` in reading order, or else the one after:
    /// the one that takes its space when it closes, in the usual case.
    pub fn adjacent(&self, pane: &T) -> Option<T> {
        self.adjacent_in_order(pane, Direction::Left)
            .or_else(|| self.adjacent_in_order(pane, Direction::Right))
    }

    fn adjacent_in_order(&self, from: &T, direction: Direction) -> Option<T> {
        let panes = self.panes();
        let index = panes.iter().position(|pane| pane == from)?;
        match direction {
            Direction::Left => index.checked_sub(1).map(|index| panes[index].clone()),
            Direction::Right => panes.get(index + 1).cloned(),
        }
    }
}

const EPSILON: f32 = 1e-4;

fn is_beside(current: &Rect, other: &Rect, direction: Direction) -> bool {
    let on_side = match direction {
        Direction::Left => other.right() <= current.x + EPSILON,
        Direction::Right => other.x >= current.right() - EPSILON,
    };
    on_side && other.vertical_overlap(current) > EPSILON
}

fn side_distance(current: &Rect, other: &Rect, direction: Direction) -> f32 {
    match direction {
        Direction::Left => current.x - other.right(),
        Direction::Right => other.x - current.right(),
    }
}

fn collect_leaves<T: Clone>(node: &Node<T>, out: &mut Vec<T>) {
    match node {
        Node::Leaf(pane) => out.push(pane.clone()),
        Node::Split(split) => {
            collect_leaves(&split.first, out);
            collect_leaves(&split.second, out);
        }
    }
}

fn collect_rects<T: Clone>(node: &Node<T>, rect: Rect, out: &mut Vec<(T, Rect)>) {
    match node {
        Node::Leaf(pane) => out.push((pane.clone(), rect)),
        Node::Split(split) => {
            let (first, second) = rect.divide(split.axis, split.ratio);
            collect_rects(&split.first, first, out);
            collect_rects(&split.second, second, out);
        }
    }
}

fn find_leaf_mut<'a, T: PartialEq>(node: &'a mut Node<T>, target: &T) -> Option<&'a mut Node<T>> {
    match node {
        Node::Leaf(pane) if pane == target => Some(node),
        Node::Leaf(_) => None,
        Node::Split(split) => {
            if let Some(found) = find_leaf_mut(&mut split.first, target) {
                return Some(found);
            }
            find_leaf_mut(&mut split.second, target)
        }
    }
}

fn find_split<T>(node: &Node<T>, id: SplitId) -> Option<&Split<T>> {
    let Node::Split(split) = node else {
        return None;
    };
    if split.id == id {
        return Some(split);
    }
    find_split(&split.first, id).or_else(|| find_split(&split.second, id))
}

fn find_split_mut<T>(node: &mut Node<T>, id: SplitId) -> Option<&mut Split<T>> {
    let Node::Split(split) = node else {
        return None;
    };
    if split.id == id {
        return Some(split);
    }
    if let Some(found) = find_split_mut(&mut split.first, id) {
        return Some(found);
    }
    find_split_mut(&mut split.second, id)
}

fn is_leaf<T: PartialEq>(node: &Node<T>, target: &T) -> bool {
    matches!(node, Node::Leaf(pane) if pane == target)
}

/// `node` without `target`: the split that held it becomes its sibling.
fn remove_from<T: PartialEq>(node: Node<T>, target: &T) -> (Node<T>, bool) {
    let Node::Split(mut split) = node else {
        return (node, false);
    };
    if is_leaf(&split.first, target) {
        return (*split.second, true);
    }
    if is_leaf(&split.second, target) {
        return (*split.first, true);
    }
    let (first, removed) = remove_from(*split.first, target);
    split.first = Box::new(first);
    if removed {
        return (Node::Split(split), true);
    }
    let (second, removed) = remove_from(*split.second, target);
    split.second = Box::new(second);
    (Node::Split(split), removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitting_and_removing_keeps_order() {
        let mut tree = PaneTree::new(1);
        assert!(tree.split(&1, 2, Axis::Row));
        assert!(tree.split(&2, 3, Axis::Column));
        assert_eq!(tree.panes(), vec![1, 2, 3]);
        assert!(tree.remove(&2));
        assert_eq!(tree.panes(), vec![1, 3]);
        assert!(tree.remove(&1));
        assert_eq!(tree.panes(), vec![3]);
        assert!(!tree.remove(&3));
        assert!(!tree.split(&9, 4, Axis::Row));
    }

    #[test]
    fn focus_moves_to_the_side_then_through_stacks() {
        // 1 | (2 over 3)
        let mut tree = PaneTree::new(1);
        tree.split(&1, 2, Axis::Row);
        tree.split(&2, 3, Axis::Column);
        assert_eq!(tree.neighbor(&1, Direction::Right), Some(2));
        assert_eq!(tree.neighbor(&3, Direction::Left), Some(1));
        assert_eq!(tree.neighbor(&2, Direction::Right), Some(3));
        assert_eq!(tree.neighbor(&3, Direction::Right), None);
        assert_eq!(tree.neighbor(&1, Direction::Left), None);
    }

    #[test]
    fn ratios_are_clamped() {
        let mut tree = PaneTree::new(1);
        tree.split(&1, 2, Axis::Row);
        let Node::Split(split) = tree.root() else {
            panic!("expected a split");
        };
        let id = split.id;
        tree.set_ratio(id, 0.99);
        assert_eq!(tree.split_by_id(id).unwrap().ratio, 1. - MIN_RATIO);
        let rects = tree.rects();
        assert!((rects[1].1.x - (1. - MIN_RATIO)).abs() < 1e-6);
    }
}
