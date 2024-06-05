use serde::{Deserialize, Serialize};

/// A piece of the text. Generally representing a detokenized token.
// In the future this may contain per-piece metadata.
#[derive(Serialize, Deserialize)]
pub struct Piece {
    /// End index of the piece (start is the end of the previous piece).
    pub end: usize,
}

static_assertions::assert_impl_all!(Piece: Send, Sync);

/// Node data. Contains a paragraph within a story tree.
#[derive(Default, Serialize, Deserialize)]
pub struct Node<T> {
    /// Author id.
    pub author_id: u8,
    /// The text of the paragraph.
    pub text: String,
    /// Piece indices.
    pub pieces: Vec<Piece>,
    /// The children of this node.
    pub children: Vec<Node<T>>,
    /// Metadata.
    #[serde(default)]
    pub meta: T,
}

static_assertions::assert_impl_all!(Node<Meta>: Send, Sync);

/// Node metadata.
#[derive(Clone, Serialize, Deserialize)]
#[cfg(feature = "gui")]
pub struct Meta {
    /// Node id.
    pub(crate) id: u128,
    /// Node position (top left).
    pub pos: egui::Pos2,
    /// Node size.
    pub size: egui::Vec2,
}

#[cfg(feature = "gui")]
impl Meta {
    /// Get unique id.
    pub fn id(&self) -> u128 {
        self.id
    }
}

#[cfg(feature = "gui")]
impl Default for Meta {
    fn default() -> Self {
        let id = uuid::Uuid::new_v4().as_u128();
        Self {
            id,
            pos: egui::Pos2::new(0.0, 0.0),
            size: egui::Vec2::new(100.0, 100.0),
        }
    }
}

/// An action is needed for a node. All actions imply selection of either the
/// current node or a child node.
#[cfg(feature = "gui")]
#[derive(Default)]
pub struct Action {
    /// The node should be deleted.
    pub delete: bool,
    /// Generation should continue within this node.
    pub continue_: bool,
    /// If new node should be generated, and it's child index.
    pub generate: Option<usize>,
    /// If the node (or tree) has been modified. This is an optimization to
    /// avoid unnecessary rendering, allocation, and node traversal.
    pub modified: bool,
}

#[cfg(feature = "gui")]
static_assertions::assert_impl_all!(Action: Send, Sync);

#[cfg(feature = "gui")]
impl Action {
    /// Returns true if any action is needed.
    pub fn action_needed(&self) -> bool {
        self.continue_ || self.generate.is_some()
    }
}

/// An action is needed at a node path.
#[cfg(feature = "gui")]
#[derive(Default)]
pub struct PathAction {
    /// A path.
    pub path: Vec<usize>,
    /// The action(s) to take on the selected path.
    pub action: Action,
}

#[cfg(feature = "gui")]
static_assertions::assert_impl_all!(PathAction: Send, Sync);

/// Dummy node metadata.
#[derive(Default, Serialize, Deserialize)]
#[cfg(not(feature = "gui"))]
pub struct Meta;

impl<T> Node<T> {
    /// Create a new node with author id.
    pub fn with_author(author_id: u8) -> Self
    where
        T: Default,
    {
        Self {
            author_id,
            ..Self::default()
        }
    }

    /// Returns true if the node has no children.
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }

    /// Adds a child to self. Returns the index of the child.
    pub fn add_child(&mut self, child: Node<T>) -> usize {
        self.children.push(child);
        self.children.len() - 1
    }

    /// Returns true if a path is valid.
    pub fn is_valid_path(&self, path: &[usize]) -> bool {
        let mut node = self;
        for &i in path {
            if i >= node.children.len() {
                return false;
            }
            node = &node.children[i];
        }
        true
    }

    /// Extend self with pieces, as strings, from an iterator.
    pub fn extend_strings<I, S>(&mut self, strings: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut start = self.text.len();
        for string in strings {
            let text: String = string.into();
            let end = start + text.len();
            self.text.push_str(&text);
            self.pieces.push(Piece { end });
            start = end;
        }
    }

    /// Iterate nodes over a path, including self.
    ///
    /// If a part of a path is invalid, the iteration will stop at the last
    /// valid node.
    pub fn iter_path_nodes<'a>(
        &'a self,
        path: &'a [usize],
    ) -> impl Iterator<Item = &'a Node<T>> + 'a {
        let mut node = Some(self);
        std::iter::once(self).chain(path.iter().filter_map(move |&i| {
            if let Some(n) = node {
                node = n.children.get(i);
                node
            } else {
                None
            }
        }))
    }

    /// Iterate all nodes in the tree in breadth-first order.
    pub fn iter_breadth_first<'a>(
        &'a self,
    ) -> impl Iterator<Item = &'a Node<T>> + 'a {
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(self);
        std::iter::from_fn(move || {
            if let Some(node) = queue.pop_front() {
                queue.extend(node.children.iter());
                Some(node)
            } else {
                None
            }
        })
    }

    /// Iterate all nodes in the tree in depth-first order.
    pub fn iter_depth_first<'a>(
        &'a self,
    ) -> impl Iterator<Item = &'a Node<T>> + 'a {
        // This is allowed in rust, but there is the risk of stack overflow :/
        // std::iter::once(self).chain(self.children.iter().map(Self::iter_breadth_first).flatten())
        let mut stack = vec![self];
        std::iter::from_fn(move || {
            if let Some(node) = stack.pop() {
                stack.extend(node.children.iter().rev());
                Some(node)
            } else {
                None
            }
        })
    }

    /// Iterate Pieces of the node as strings.
    pub fn iter_pieces<'a>(&'a self) -> impl Iterator<Item = &'a str> + 'a {
        self.pieces
            .iter()
            .map(|p| p.end)
            .scan(0, move |start, end| {
                let text = self.text.get(*start..end);
                *start = end;
                text
            })
    }

    /// Iterate text over a path, including self, joining each node with a
    /// separator.
    ///
    /// # Panics
    /// - If the path is invalid.
    pub fn iter_path_text<'a>(
        &'a self,
        path: &'a [usize],
        separator: &'a str,
    ) -> impl Iterator<Item = &str> + 'a {
        self.iter_path_nodes(path)
            .map(move |node| {
                std::iter::once(separator).chain(node.iter_pieces())
            })
            .flatten()
            .skip(1)
    }

    /// Trim whitespace from the end of the text and adjust the pieces.
    pub fn trim_end_whitespace(&mut self) {
        let len = self.text.trim_end().len();
        self.text.truncate(len);
        while let Some(piece) = self.pieces.last() {
            if piece.end > len {
                self.pieces.pop();
            } else {
                break;
            }
        }
        // finally, we need to insert a new piece if the last one is not at the
        // end of the text
        if self.pieces.last().map_or(true, |p| p.end != len) {
            self.pieces.push(Piece { end: len });
        }
    }
}

impl<T> std::fmt::Display for Node<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for piece in self.iter_pieces() {
            write!(f, "{}", piece)?;
        }
        Ok(())
    }
}

impl Node<Meta> {
    /// Draw the tree as nodes. The active path is highlighted. If
    /// `lock_topology` is true, the user cannot add or remove nodes.
    ///
    /// Returns an action to perform at the path or None if no action is needed.
    #[cfg(feature = "gui")]
    pub fn draw_nodes(
        &mut self,
        ui: &mut egui::Ui,
        active_path: Option<&[usize]>,
        lock_topology: bool,
    ) -> Option<PathAction> {
        let active_path = active_path.unwrap_or(&[]);
        let mut ret = None; // the default, meaning no action is needed.

        // The current path in the tree.
        let mut current_path = Vec::new();

        // The stack data is:
        // * The node index
        // * The node itself
        // * The depth of the node
        // * Whether the node is in the active path
        let mut stack = Vec::new();
        stack.push((0, self, 0, true));

        // Do a depth-first traversal of the tree.
        while let Some((i, node, depth, highlight_node)) = stack.pop() {
            if depth != 0 {
                // Update the current path.
                if current_path.len() < depth {
                    // Append from the parent node.
                    current_path.push(i);
                } else {
                    // Update the current path depth to match.
                    current_path[depth - 1] = i;
                    current_path.truncate(depth);
                }
            }

            // Draw the node and take any action in response to it's widgets.
            if let Some(action) =
                node.draw_one_node(ui, highlight_node, lock_topology)
            {
                if action.delete {
                    // How to delete a node? We're taking a reference to the
                    // node so we can't delete it here. We can delete the
                    // children, but we can't delete the node itself -- at least
                    // not here. So well forward the action to the caller which
                    // can delete this node, and with it, all its children.
                    ret = Some(PathAction {
                        path: current_path.clone(),
                        action,
                    });
                    // We don't need to draw the children of this node, so we
                    // continue to the next node.
                    continue;
                }

                if let Some(child_index) = action.generate {
                    // Append the (new) child index to the path and tell the
                    // caller to generate from the new child.
                    let mut path = current_path.clone();
                    path.push(child_index);

                    // The caller has some action to perform at the path.
                    ret = Some(PathAction { path, action });
                } else {
                    // Any other action doesn't require changing the path.
                    ret = Some(PathAction {
                        path: current_path.clone(),
                        action,
                    });
                }
            }

            for (j, child) in node.children.iter_mut().enumerate() {
                // Highlight this child if it is in the active path.
                let highlight_child = highlight_node
                    && active_path
                        .get(depth)
                        .is_some_and(|&active_index| j == active_index);

                // Draw the line from the parent to the child.
                let src = node.meta.clone();
                let dst = child.meta.clone();
                draw_line(ui, src, dst, highlight_child);

                // Push the child to the stack.
                stack.push((j, child, depth + 1, highlight_child));
            }
        }

        ret
    }

    /// Helper for draw functions to draw just the buttons.
    #[cfg(feature = "gui")]
    pub fn draw_buttons(
        &mut self,
        ui: &mut egui::Ui,
        action: &mut Option<Action>,
    ) -> egui::Response {
        let resp = ui.horizontal(|ui| {
            let add_child = ui
                .button("Add Child")
                .on_hover_text_at_pointer("Add an empty child node.");
            if add_child.clicked() {
                self.add_child(Node::default());
            }
            let delete = ui.button("Delete").on_hover_text_at_pointer(
                "Delete this node and all its children.",
            );
            if delete.clicked() {
                // Tell caller to delete this node.
                *action = Some(Action {
                    delete: true,
                    ..Default::default()
                });
            }
            // FIXME: The terminology here could be improved. These are
            // confusing. We should find new names.
            let continue_ = ui.button("Continue").on_hover_text_at_pointer(
                "Continue generating the current node.",
            );
            if continue_.clicked() {
                // Tell caller to continue generation on this node.
                *action = Some(Action {
                    continue_: true,
                    ..Default::default()
                });
            }
            let generate = ui.button("Generate").on_hover_text_at_pointer(
                "Create a new node, select it, and continue generation.",
            );
            if generate.clicked() {
                // Tell caller to generate a new node.
                *action = Some(Action {
                    generate: Some(self.add_child(Node::default())),
                    ..Default::default()
                });
            }

            add_child | delete | continue_ | generate
        });

        let resp = resp.response | resp.inner;
        if resp.clicked() && action.is_none() {
            // Any click should select the node. If action is_some at all, it
            // means the node should be selected, unless topology is locked, but
            // that's handled elsewhere.
            *action = Some(Action::default());
        }
        resp
    }

    /// Helper for draw functions to draw just the text edit.
    #[cfg(feature = "gui")]
    pub fn draw_text_edit(
        &mut self,
        ui: &mut egui::Ui,
        action: &mut Option<Action>,
    ) -> egui::Response {
        // We can still allow editing the text during generation since
        // the pieces are still appended to the end. There is no
        // ownership issue because of the immediate mode GUI.
        let resp = ui.text_edit_multiline(&mut self.text);
        if resp.changed() {
            // There has been a modification to the text. We need to update
            // the modification flag so cached data is invalidated.
            // FIXME: We're clearing the pieces here, but we can handle
            // this better.
            self.pieces.clear();
            self.pieces.push(Piece {
                end: self.text.len(),
            });
            if let Some(action) = action {
                action.modified = true;
            } else {
                let mut a = Action::default();
                a.modified = true;
                *action = Some(a);
            }
        }
        if resp.clicked() && action.is_none() {
            *action = Some(Action::default());
        }

        resp
    }

    /// Draw just the node. Returns true if the node should be active.
    #[cfg(feature = "gui")]
    pub fn draw_one_node(
        &mut self,
        ui: &mut egui::Ui,
        highlighted: bool,
        lock_topology: bool,
    ) -> Option<Action> {
        let frame = egui::Frame::window(&ui.ctx().style())
            .fill(egui::Color32::from_gray(64));

        let title = self
            .text
            .chars()
            .take(16)
            .chain(std::iter::once('…'))
            .collect::<String>();

        let mut response = egui::Window::new(&title)
            .id(egui::Id::new(self.meta.id))
            .collapsible(true)
            .title_bar(true)
            .auto_sized()
            .frame(frame)
            .show(ui.ctx(), |ui| {
                if highlighted {
                    ui.set_opacity(1.5);
                } else {
                    ui.set_opacity(0.5);
                }

                let mut action = None;
                if !lock_topology {
                    self.draw_buttons(ui, &mut action);
                }

                // We can still allow editing the text during generation since
                // the pieces are still appended to the end. There is no
                // ownership issue because of the immediate mode GUI and there
                // are no topology changes so the new tokens are appended at the
                // correct path.
                self.draw_text_edit(ui, &mut action);

                action
            });

        if let Some(response) = &mut response {
            if let Some(inner) = response.inner.as_mut() {
                // If the window was clicked, we need to select the node.
                if inner.is_none() && response.response.clicked() {
                    // If the window was clicked, we need to select the node.
                    inner.replace(Action::default());
                }
            }
        }

        // If the window has been interacted with, we need to store the new size
        // and position. We also need to forward any inner activation response
        // from the closure above to the caller.
        if let Some(response) = response {
            // Response from the *window*.
            let win = response.response;

            self.meta.pos = win.rect.min;
            self.meta.size = win.rect.size();

            // Unwrap inner response from the closure and send it to the caller
            // letting the caller know if any action is needed.
            response.inner.unwrap_or(None)
        } else {
            None
        }
    }

    /// Draw the tree.
    #[cfg(feature = "gui")]
    pub fn draw(
        &mut self,
        ui: &mut egui::Ui,
        selected_path: Option<&[usize]>,
        lock_topology: bool,
        mode: crate::story::DrawMode,
    ) -> Option<PathAction> {
        use crate::story::DrawMode;

        match mode {
            DrawMode::Nodes => {
                self.draw_nodes(ui, selected_path, lock_topology)
            }
            DrawMode::Tree => {
                let auto_collapse = ui
                    .button("auto-collapse")
                    .on_hover_text_at_pointer("Collapse all except selected.")
                    .clicked();

                egui::ScrollArea::vertical()
                    .show(ui, |ui| {
                        self.draw_tree(
                            ui,
                            selected_path,
                            None, // current path (root is None)
                            0,    // depth
                            true, // selected
                            auto_collapse,
                            lock_topology,
                        )
                    })
                    .inner
            }
        }
    }

    /// A helper function to draw the tree as collapsible headers.
    ///
    /// - `ui`: The egui context.
    /// - `selected_path`: The selected path in the tree.
    /// - `current_path`: The current path (of this node, hopefully).
    /// - `depth`: The distance from the root.
    /// - `selected`: Whether this node is selected.
    /// - `auto_collapse`: Whether to auto-collapse nodes. If the node is
    ///   selected, it will be opened, if not, it will be closed.
    /// - `lock_topology`: Whether the topology is locked. Disables buttons
    ///   that change topology. Editing text is still allowed.
    #[cfg(feature = "gui")]
    fn draw_tree(
        &mut self,
        ui: &mut egui::Ui,
        selected_path: Option<&[usize]>,
        current_path: Option<Vec<usize>>,
        depth: usize,
        selected: bool,
        auto_collapse: bool,
        lock_topology: bool,
    ) -> Option<PathAction> {
        let title = self
            .text
            .chars()
            .take(16)
            .chain(std::iter::once('…'))
            .collect::<String>();

        let open = if selected {
            Some(true)
        } else {
            if auto_collapse {
                Some(false)
            } else {
                None
            }
        };

        // This is a recursive implementation rather than using a stack like
        // above because it suits the egui API better (nested elements). It's
        // very unlikely that the depth of the tree will be so large that it
        // will cause a stack overflow. It's also prettier and easier to
        // understand.
        egui::CollapsingHeader::new(title)
            .default_open(open.unwrap_or(false))
            .open(open)
            .id_source(egui::Id::new(("tree", self.meta.id)))
            .show(ui, |ui| {
                let mut action: Option<Action> = None;
                let mut path_action = None;

                if selected {
                    ui.set_opacity(1.0);
                } else {
                    ui.set_opacity(0.5);
                }

                // Draw buttons
                if !lock_topology {
                    self.draw_buttons(ui, &mut action);
                }

                // Draw text edit
                self.draw_text_edit(ui, &mut action);

                for (i, child) in self.children.iter_mut().enumerate() {
                    let mut child_path =
                        current_path.clone().unwrap_or_default();
                    child_path.push(i);
                    let selected = selected
                        && selected_path
                            .is_some_and(|p| p.get(depth) == Some(&i));
                    if let Some(a) = child.draw_tree(
                        ui,
                        selected_path,
                        Some(child_path),
                        depth + 1,
                        selected,
                        auto_collapse,
                        lock_topology,
                    ) {
                        path_action = Some(a);
                    }
                }

                if let Some(action) = action {
                    Some(PathAction {
                        path: current_path.unwrap_or_default(),
                        action,
                    })
                } else {
                    path_action
                }
            })
            .body_returned?
    }
}

/// Draw a line between two nodes.
#[cfg(feature = "gui")]
fn draw_line(ui: &mut egui::Ui, src: Meta, dst: Meta, highlighted: bool) {
    let color = if highlighted {
        egui::Color32::from_rgba_premultiplied(255, 255, 255, 255)
    } else {
        egui::Color32::from_rgba_premultiplied(128, 128, 128, 255)
    };
    let stroke = egui::Stroke::new(if highlighted { 2.0 } else { 1.0 }, color);
    let src = src.pos + src.size / 2.0;
    let dst = dst.pos + dst.size / 2.0;
    ui.painter().line_segment([src, dst], stroke);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iter_path_nodes() {
        let mut root = Node::<Meta>::default();
        root.extend_strings(vec!["a", "b"]);
        assert_eq!(0, root.add_child(Node::default()));
        root.children[0].extend_strings(vec!["c", "d"]);
        assert_eq!(0, root.children[0].add_child(Node::default()));
        root.children[0].children[0].extend_strings(vec!["e", "f"]);

        let path = [0, 0];
        let nodes: Vec<_> = root.iter_path_nodes(&path).collect();
        let letters = nodes.iter().flat_map(|node| node.iter_pieces());
        assert_eq!(letters.collect::<String>(), "abcdef");
    }

    #[test]
    fn iter_path_text() {
        let mut root = Node::<Meta>::default();
        root.extend_strings(vec!["a", "b"]);
        assert_eq!(0, root.add_child(Node::default()));
        root.children[0].extend_strings(vec!["c", "d"]);
        assert_eq!(0, root.children[0].add_child(Node::default()));
        root.children[0].children[0].extend_strings(vec!["e", "f"]);

        let path = [0, 0];
        let text: Vec<_> = root.iter_path_text(&path, " ").collect();
        assert_eq!(text.len(), 8);
        assert_eq!(text[0], "a");
        assert_eq!(text[1], "b");
        assert_eq!(text[2], " ");
        assert_eq!(text[3], "c");
        assert_eq!(text[4], "d");
        assert_eq!(text[5], " ");
        assert_eq!(text[6], "e");
        assert_eq!(text[7], "f");
    }

    #[test]
    fn test_is_valid_path() {
        let mut root = Node::<Meta>::default();
        root.extend_strings(vec!["a", "b"]);
        assert_eq!(0, root.add_child(Node::default()));
        root.children[0].extend_strings(vec!["c", "d"]);
        assert_eq!(0, root.children[0].add_child(Node::default()));
        root.children[0].children[0].extend_strings(vec!["e", "f"]);

        assert!(root.is_valid_path(&[0]));
        assert!(root.is_valid_path(&[0, 0]));
        assert!(!root.is_valid_path(&[1]));
        assert!(!root.is_valid_path(&[0, 1]));
    }

    #[test]
    fn test_iter_breadth_first() {
        let mut root = Node::<Meta>::default();
        root.extend_strings(vec!["a", "b"]);
        assert_eq!(0, root.add_child(Node::default()));
        root.children[0].extend_strings(vec!["c", "d"]);
        assert_eq!(1, root.add_child(Node::default()));
        root.children[1].extend_strings(vec!["e", "f"]);
        assert_eq!(0, root.children[0].add_child(Node::default()));
        root.children[0].children[0].extend_strings(vec!["g", "h"]);

        let nodes: Vec<_> = root.iter_breadth_first().collect();
        let letters = nodes.iter().flat_map(|node| node.iter_pieces());
        assert_eq!(letters.collect::<String>(), "abcdefgh");
    }

    #[test]
    fn test_iter_depth_first() {
        let mut root = Node::<Meta>::default();
        root.extend_strings(vec!["a", "b"]);
        assert_eq!(0, root.add_child(Node::default()));
        root.children[0].extend_strings(vec!["c", "d"]);
        assert_eq!(1, root.add_child(Node::default()));
        root.children[1].extend_strings(vec!["e", "f"]);
        assert_eq!(0, root.children[0].add_child(Node::default()));
        root.children[0].children[0].extend_strings(vec!["g", "h"]);

        let nodes: Vec<_> = root.iter_depth_first().collect();
        assert_eq!(nodes.len(), 4);
        let letters = nodes.iter().flat_map(|node| node.iter_pieces());
        assert_eq!(letters.collect::<String>(), "abcdghef");
    }
}
