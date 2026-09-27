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

/// A direction to move focus or a tab in, or a side to split toward.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    /// The axis a split toward this side divides along.
    pub fn axis(self) -> Axis {
        match self {
            Direction::Left | Direction::Right => Axis::Row,
            Direction::Up | Direction::Down => Axis::Column,
        }
    }

    /// Whether a pane put on this side comes first in reading order.
    fn is_before(self) -> bool {
        matches!(self, Direction::Left | Direction::Up)
    }
}

/// Where a tab dropped on a pane's note lands: in the pane, or in a new
/// pane split off toward one side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropZone {
    Centre,
    Side(Direction),
}

/// How far in from a pane's edge, as a share of its size, a drop splits
/// the pane rather than joining it.
const EDGE_SHARE: f32 = 0.3;

impl DropZone {
    /// The zone under a point given as shares of the pane's width and
    /// height: the nearest edge when it's within [`EDGE_SHARE`], or else
    /// the centre.
    pub fn at(x: f32, y: f32) -> DropZone {
        let edges = [
            (x, Direction::Left),
            (1. - x, Direction::Right),
            (y, Direction::Up),
            (1. - y, Direction::Down),
        ];
        edges
            .into_iter()
            .filter(|(distance, _)| *distance < EDGE_SHARE)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map_or(DropZone::Centre, |(_, side)| DropZone::Side(side))
    }

    /// Where the tab will show, as a share of the pane: the half on that
    /// side, or the whole pane.
    pub fn landing(self) -> Rect {
        match self {
            DropZone::Centre => Rect::UNIT,
            DropZone::Side(side) => {
                let (first, second) = Rect::UNIT.divide(side.axis(), 0.5);
                if side.is_before() { first } else { second }
            }
        }
    }
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
    pub const UNIT: Rect = Rect {
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

    fn horizontal_overlap(&self, other: &Rect) -> f32 {
        (self.right().min(other.right()) - self.x.max(other.x)).max(0.)
    }

    /// How much of `other` lines up with this rect across `direction`:
    /// shared height for left and right, shared width for up and down.
    fn overlap_across(&self, other: &Rect, direction: Direction) -> f32 {
        match direction.axis() {
            Axis::Row => self.vertical_overlap(other),
            Axis::Column => self.horizontal_overlap(other),
        }
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
        let side = match axis {
            Axis::Row => Direction::Right,
            Axis::Column => Direction::Down,
        };
        self.split_toward(target, new, side).is_some()
    }

    /// Puts `new` on the `side` of `target`, each taking half its space.
    pub fn split_toward(&mut self, target: &T, new: T, side: Direction) -> Option<SplitId> {
        let id = SplitId(self.next_split);
        let node = find_leaf_mut(&mut self.root, target)?;
        self.next_split += 1;
        let old = std::mem::replace(node, Node::Leaf(new.clone()));
        let (first, second) = if side.is_before() {
            (Node::Leaf(new), old)
        } else {
            (old, Node::Leaf(new))
        };
        *node = Node::Split(Split {
            id,
            axis: side.axis(),
            ratio: 0.5,
            first: Box::new(first),
            second: Box::new(second),
            bounds: Rc::default(),
        });
        Some(id)
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

    /// Shares a split's space so every pane in a run along its axis gets
    /// the same room: two panes beside a third leave it a third.
    pub fn equalize(&mut self, id: SplitId) {
        if let Some(split) = find_split_mut(&mut self.root, id) {
            let first = span(&split.first, split.axis) as f32;
            let second = span(&split.second, split.axis) as f32;
            split.ratio = first / (first + second);
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
    /// side, or else, for left and right, the pane before or after it in
    /// reading order, which reaches panes stacked above and below.
    pub fn neighbor(&self, from: &T, direction: Direction) -> Option<T> {
        let in_order = || match direction.axis() {
            Axis::Row => self.adjacent_in_order(from, direction),
            Axis::Column => None,
        };
        self.beside(from, direction).or_else(in_order)
    }

    /// The nearest pane on `direction`'s side of `from` that lines up with
    /// it, preferring the one that lines up most.
    pub fn beside(&self, from: &T, direction: Direction) -> Option<T> {
        let rects = self.rects();
        let (_, current) = rects.iter().find(|(pane, _)| pane == from)?;
        rects
            .iter()
            .filter(|(pane, rect)| pane != from && is_beside(current, rect, direction))
            .min_by(|a, b| {
                side_distance(current, &a.1, direction)
                    .total_cmp(&side_distance(current, &b.1, direction))
                    .then(
                        b.1.overlap_across(current, direction)
                            .total_cmp(&a.1.overlap_across(current, direction)),
                    )
            })
            .map(|(pane, _)| pane.clone())
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
        if direction.is_before() {
            index.checked_sub(1).map(|index| panes[index].clone())
        } else {
            panes.get(index + 1).cloned()
        }
    }
}

const EPSILON: f32 = 1e-4;

fn is_beside(current: &Rect, other: &Rect, direction: Direction) -> bool {
    side_distance(current, other, direction) >= -EPSILON
        && other.overlap_across(current, direction) > EPSILON
}

/// How far `other` lies past `current`'s edge on that side; negative when
/// it isn't on that side.
fn side_distance(current: &Rect, other: &Rect, direction: Direction) -> f32 {
    match direction {
        Direction::Left => current.x - other.right(),
        Direction::Right => other.x - current.right(),
        Direction::Up => current.y - other.bottom(),
        Direction::Down => other.y - current.bottom(),
    }
}

/// How many panes sit in a run along `axis` in `node`.
fn span<T>(node: &Node<T>, axis: Axis) -> usize {
    match node {
        Node::Split(split) if split.axis == axis => {
            span(&split.first, axis) + span(&split.second, axis)
        }
        _ => 1,
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
    fn quarters_come_from_splitting_each_half() {
        // (1 over 3) | (2 over 4)
        let mut tree = PaneTree::new(1);
        tree.split_toward(&1, 2, Direction::Right);
        tree.split_toward(&1, 3, Direction::Down);
        tree.split_toward(&2, 4, Direction::Down);
        assert_eq!(tree.panes(), vec![1, 3, 2, 4]);
        for (_, rect) in tree.rects() {
            assert!((rect.width - 0.5).abs() < 1e-6 && (rect.height - 0.5).abs() < 1e-6);
        }
        assert_eq!(tree.neighbor(&1, Direction::Down), Some(3));
        assert_eq!(tree.neighbor(&4, Direction::Up), Some(2));
        assert_eq!(tree.neighbor(&3, Direction::Right), Some(4));
        assert_eq!(tree.neighbor(&1, Direction::Up), None);
        // Closing a quarter gives its room back to the pane above it.
        assert!(tree.remove(&3));
        assert_eq!(tree.panes(), vec![1, 2, 4]);
        assert!((tree.rects()[0].1.height - 1.).abs() < 1e-6);
    }

    #[test]
    fn splits_go_first_on_the_left_and_top() {
        let mut tree = PaneTree::new(1);
        tree.split_toward(&1, 2, Direction::Left);
        tree.split_toward(&1, 3, Direction::Up);
        assert_eq!(tree.panes(), vec![2, 3, 1]);
        assert_eq!(tree.beside(&1, Direction::Left), Some(2));
        assert_eq!(tree.beside(&1, Direction::Up), Some(3));
    }

    #[test]
    fn equalizing_shares_a_run_evenly() {
        // 1 | (2 | 3): the outer split leaves 1 a third.
        let mut tree = PaneTree::new(1);
        let outer = tree.split_toward(&1, 2, Direction::Right).unwrap();
        tree.split_toward(&2, 3, Direction::Right);
        tree.equalize(outer);
        assert!((tree.split_by_id(outer).unwrap().ratio - 1. / 3.).abs() < 1e-6);
    }

    #[test]
    fn drop_zones_follow_the_nearest_edge() {
        assert_eq!(DropZone::at(0.5, 0.5), DropZone::Centre);
        assert_eq!(DropZone::at(0.1, 0.5), DropZone::Side(Direction::Left));
        assert_eq!(DropZone::at(0.95, 0.4), DropZone::Side(Direction::Right));
        assert_eq!(DropZone::at(0.4, 0.05), DropZone::Side(Direction::Up));
        assert_eq!(DropZone::at(0.2, 0.9), DropZone::Side(Direction::Down));
        let left = DropZone::Side(Direction::Left).landing();
        assert_eq!((left.x, left.width, left.height), (0., 0.5, 1.));
        let down = DropZone::Side(Direction::Down).landing();
        assert_eq!((down.y, down.height), (0.5, 0.5));
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
