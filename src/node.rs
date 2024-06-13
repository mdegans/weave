use egui::Pos2;
use serde::{Deserialize, Serialize};

/// A piece of the text. Generally representing a detokenized token.
// In the future this may contain per-piece metadata.
#[derive(Serialize, Deserialize)]
pub struct Piece {
    /// End index of the piece (start is the end of the previous piece).
    pub end: usize,
}

/// Time step for the force-directed layout.
// FIXME: This should be a parameter and based on the (previous) frame time
// or perhaps the average over several frames.
const TIME_STEP: f32 = 1.0 / 60.0;
/// Damping factor for the force-directed layout.
const DAMPING: f32 = 0.10;
/// Boundary damping factor when nodes hit the boundaries and bounce back.
const BOUNDARY_DAMPING: f32 = 0.5;
/// Mass divisor for the force-directed layout.
const MASS_DIVISOR: f32 = 1000.0;
/// Padding for the bounding rectangle of the node. Also the max velocity.
const PADDING: f32 = 32.0;
/// Ratio of local to global centroid and mass. A ratio of 5 means that the
/// nodes are 5 times more attracted to the local centroid than the global
/// centroid. This also controls the repulsion from the parent node.
const LOCAL_GLOBAL_RATIO: f32 = 5.0;

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
    /// Node position (center).
    pub pos: egui::Pos2,
    /// Node size.
    pub size: egui::Vec2,
    /// Velocity.
    #[serde(skip)]
    pub vel: egui::Vec2,
}

#[cfg(feature = "gui")]
impl Meta {
    /// Get unique id.
    #[inline]
    pub fn id(&self) -> u128 {
        self.id
    }

    /// Get bounding rectangle (with padding)
    #[inline]
    pub fn rect(&self) -> egui::Rect {
        egui::Rect::from_min_size(self.pos, self.size).expand(PADDING)
    }

    /// Get mass of the node.
    #[inline]
    pub fn mass(&self) -> f32 {
        self.size.x * self.size.y / MASS_DIVISOR
    }
}

#[cfg(feature = "gui")]
impl Default for Meta {
    fn default() -> Self {
        let id = uuid::Uuid::new_v4().as_u128();
        Self {
            id,
            pos: egui::Pos2::new(0.0, 0.0),
            size: egui::Vec2::new(0.0, 0.0),
            vel: egui::Vec2::new(0.0, 0.0),
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[cfg(feature = "gui")]
/// Positional layout for the tree.
pub enum PositionalLayout {
    /// Force-directed layout.
    ForceDirected {
        /// Repulsion factor (of nodes). This is inverse square.
        repulsion: f32,
        /// Attraction factor (of edges). This is linear.
        attraction: f32,
        /// How much nodes should be attracted to the centroid. This is inverse
        /// square.
        gravity: f32,
    },
}

#[cfg(feature = "gui")]
impl PositionalLayout {
    /// Get the layout as a string.
    pub const fn as_str(&self) -> &str {
        match self {
            Self::ForceDirected { .. } => "Force Directed",
        }
    }

    /// Force-directed layout default.
    pub const fn force_directed() -> Self {
        Self::ForceDirected {
            repulsion: 125.0,
            attraction: 2.5,
            gravity: 1.0,
        }
    }

    /// UI for the layout.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> egui::Response {
        match self {
            Self::ForceDirected {
                repulsion,
                attraction,
                gravity,
            } => {
                ui.horizontal(|ui| {
                    crate::icon!(ui, "../resources/expand.png", 24.0)
                        | ui.add(egui::Slider::new(repulsion, 0.0..=250.0))
                            .on_hover_text_at_pointer(
                                "How much children repel each other.",
                            )
                })
                .response
                    | ui.horizontal(|ui| {
                        crate::icon!(ui, "../resources/contract.png", 24.0)
                            | ui.add(egui::Slider::new(attraction, 0.0..=5.0))
                                .on_hover_text_at_pointer(
                                    "How much nodes attract by edges attract.",
                                )
                    })
                    .response
                    | ui.horizontal(|ui| {
                        crate::icon!(ui, "../resources/gravity.png", 24.0)
                            | ui.add(egui::Slider::new(gravity, 0.0..=5.0))
                                .on_hover_text_at_pointer(
                                "How much nodes are attracted to a weighted average of global and local centroids.",
                            )
                    })
                    .response
            }
        }
    }

    /// Apply one iteration of force-directed layout to the node. Window
    /// `bounds` should be supplied to keep the nodes within the window.
    ///
    /// If `debug` is supplied, the bounding rectangles of the nodes as well as
    /// some other debug information will be drawn.
    ///
    /// Returns true if redraw is needed.
    pub fn apply(
        self,
        node: &mut Node<Meta>,
        bounds: egui::Rect,
        debug: Option<&mut egui::Ui>,
        global_centroid: Pos2,
        global_cum_mass: f32,
    ) -> bool {
        let mut redraw = false;

        match self {
            Self::ForceDirected {
                repulsion,
                attraction,
                gravity,
            } => {
                // The general idea is for nodes to repel each other with
                // inverse square force and attract to each other with linear
                // force where an edge is present. If nodes overlap, the force
                // is reversed. The nodes also bounce off the boundaries.

                // We avoid quadratic complexity by only calculating the force
                // between node and siblings and siblings with each other.
                // This means that forces between cousins are not calculated,
                // but it's good enough for a tree.

                // Thank you, Bing's Copilot for pointing out that I was missing
                // the time step here. Also for pointing out that I was using
                // the distance between child and node to calculate force for
                // siblings below.

                // There is also a global and local centroid and mass. The nodes
                // are attracted to a weighted average of these centroids. This
                // is to keep the tree centered and balanced.

                let mut stack = vec![(node, None)];
                while let Some((node, parent_meta)) = stack.pop() {
                    // Apply damping to the velocity.
                    node.meta.vel *= 1.0 - DAMPING;

                    // This node's mass and bounding rectangle.
                    let mass = node.meta.mass();
                    let rect = node.meta.rect();

                    // In debug mode, draw the bounding rectangle of the node.
                    if let Some(ref ui) = debug {
                        egui::Area::new(egui::Id::new(("area", node.meta.id)))
                            .show(ui.ctx(), |ui| {
                                ui.painter().rect_filled(
                                    rect,
                                    0.0,
                                    egui::Color32::from_rgba_premultiplied(
                                        64, 0, 0, 128,
                                    ),
                                );
                            });
                    }

                    // The local centroid and cumulative mass (just self and
                    // children)
                    let mut centroid = node.meta.pos;
                    // FIXME: when calculating the centroid we should also
                    // take the mass into account. Currently we're just
                    // averaging the positions which is wrong. It does converge
                    // and it's good enough for now, but it's wrong. Thank you
                    // ChatGPT 4o for pointing this out. A correct example
                    // is in the `Node::centroid` method.
                    let mut cum_mass = mass;

                    // Child-to-child interactions. They repel each other. Since
                    // they do not have edges, they do not attract each other.
                    for i in 0..node.children.len() {
                        let a_mass = node.children[i].meta.mass();

                        // Accumulate the local centroid and cumulative mass.
                        centroid += node.children[i].meta.pos.to_vec2();
                        cum_mass += a_mass;

                        for j in 0..node.children.len() {
                            if i == j {
                                continue;
                            }

                            let b = &node.children[j];
                            let b_mass = b.meta.mass();

                            let dist =
                                node.children[i].meta.pos.distance(b.meta.pos);
                            let force = repulsion * a_mass * b_mass
                                / dist.powi(2)
                                * (node.children[i].meta.pos - b.meta.pos)
                                    .normalized();

                            // Children always repel each other.
                            node.children[i].meta.vel += force * TIME_STEP;
                        }

                        // Repel parent node (if any)
                        if let Some(ref parent) = parent_meta {
                            let parent: &Meta = parent;
                            let dist =
                                node.children[i].meta.pos.distance(parent.pos);
                            let force = repulsion * a_mass * mass
                                / dist.powi(2)
                                * (node.children[i].meta.pos - parent.pos)
                                    .normalized();
                            // Repulsion from parent should be stronger. This
                            // helps make the tree more balanced and tree-like.
                            node.children[i].meta.vel +=
                                force * LOCAL_GLOBAL_RATIO * TIME_STEP;
                            cum_mass += parent.mass();
                            centroid += parent.pos.to_vec2();
                        }
                    }

                    // plus 2 for self and the parent node.
                    centroid = centroid
                        / (node.children.len() as f32
                            + (if parent_meta.is_some() { 2.0 } else { 1.0 }));

                    // In debug mode, draw the local centroid.
                    if gravity > 0.0 {
                        if let Some(ref ui) = debug {
                            egui::Area::new(egui::Id::new((
                                "centroid",
                                node.meta.id,
                            )))
                            .show(
                                ui.ctx(),
                                |ui| {
                                    ui.painter().circle(
                                        centroid,
                                        gravity * cum_mass / 10.0,
                                        egui::Color32::from_rgba_premultiplied(
                                            128, 0, 0, 128,
                                        ),
                                        egui::Stroke::NONE,
                                    );
                                },
                            );
                        }
                    }

                    // Our final centroid and cumulative mass is a weighted
                    // average of the local and global centroids and masses.
                    // This is hella approximate, but it works.
                    centroid = (centroid * (LOCAL_GLOBAL_RATIO - 1.0)
                        + global_centroid.to_vec2())
                        / LOCAL_GLOBAL_RATIO;
                    cum_mass = (cum_mass * (LOCAL_GLOBAL_RATIO - 1.0)
                        + global_cum_mass)
                        / LOCAL_GLOBAL_RATIO;

                    if gravity > 0.0 && !rect.contains(centroid) {
                        // In debug builds, draw the final centroid
                        if let Some(ref ui) = debug {
                            egui::Area::new(egui::Id::new((
                                "centroid",
                                node.meta.id,
                            )))
                            .show(
                                ui.ctx(),
                                |ui| {
                                    ui.painter().circle(
                                        centroid,
                                        gravity * cum_mass / 10.0,
                                        egui::Color32::from_rgba_premultiplied(
                                            128, 128, 0, 128,
                                        ),
                                        egui::Stroke::NONE,
                                    );
                                },
                            );
                        }

                        let dist = node.meta.pos.distance(centroid);
                        let force = gravity * mass * cum_mass / dist.powi(2)
                            * (centroid - node.meta.pos).normalized();
                        node.meta.vel += force * TIME_STEP;
                    }

                    // Bounce off the boundaries. Thanks to Bing's Copilot for
                    // suggesting this. I used the same idea below for the
                    // node colissions.
                    let new_pos = egui::Rect::from_center_size(
                        node.meta.pos + node.meta.vel,
                        node.meta.size,
                    );
                    if !bounds.contains_rect(new_pos) {
                        node.meta.vel = -node.meta.vel * BOUNDARY_DAMPING;
                    }

                    // DAMPING is also used as a cutoff for velocity. If the
                    // Node isn't moving, we don't need to update the position.
                    // If no nodes are moving, we don't need to redraw. At that
                    // point the simulation has converged.
                    if node.meta.vel.normalized().abs().max_elem()
                        >= (DAMPING / 10.0)
                    {
                        node.meta.vel = node.meta.vel.clamp(
                            egui::Vec2::splat(-PADDING),
                            egui::Vec2::splat(PADDING),
                        );
                        node.meta.pos += node.meta.vel;
                        node.meta.pos =
                            node.meta.pos.clamp(bounds.min, bounds.max);

                        // If the node has moved, we need to redraw.
                        redraw = true;
                    }

                    // Child-to-node interactions. They attract each other.
                    // They do have edges so they also repel each other.
                    for child in node.children.iter_mut() {
                        // Attract to node.
                        let child_mass = child.meta.mass();
                        let child_rect = child.meta.rect();
                        let dist = node.meta.pos.distance(child.meta.pos);
                        let attraction_force = attraction * mass * child_mass
                            / dist
                            * (node.meta.pos - child.meta.pos).normalized();
                        let repulsion_force = repulsion * mass * child_mass
                            / dist.powi(2)
                            * (node.meta.pos - child.meta.pos).normalized();
                        let force = attraction_force - repulsion_force;

                        if !rect.intersects(child_rect) {
                            child.meta.vel += force * TIME_STEP;
                        } else {
                            child.meta.vel -= force * TIME_STEP;
                            child.meta.vel *= BOUNDARY_DAMPING;
                        }

                        // Recurse into the child.
                        stack.push((child, Some(node.meta.clone())));
                    }
                }
            }
        }

        redraw
    }
}

#[cfg(feature = "gui")]
impl Default for PositionalLayout {
    fn default() -> Self {
        Self::force_directed()
    }
}

#[cfg(feature = "gui")]
impl PartialEq for PositionalLayout {
    /// We only need to compare the variant. This is because we're using it in
    /// a combo box below and we don't compare it anywhere else. This is "bad",
    /// but it's fine for now.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::ForceDirected { .. }, Self::ForceDirected { .. }) => true,
        }
    }
}

/// Layout for the tree.
#[derive(Clone, Copy, Serialize, Deserialize)]
#[cfg(feature = "gui")]
pub struct Layout {
    /// Auto-collapse all nodes except the selected path.
    auto_collapse: bool,
    /// Positional layout.
    positional: Option<PositionalLayout>,
}

#[cfg(feature = "gui")]
impl Default for Layout {
    fn default() -> Self {
        Self {
            auto_collapse: false,
            positional: None,
        }
    }
}

#[cfg(feature = "gui")]
impl Layout {
    /// UI for the layout.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.toggle_value(&mut self.auto_collapse, "auto-collapse")
                .on_hover_text_at_pointer(
                    "Collapse all nodes except selected. Note that for the moment this only works for existing nodes in the tree view.",
                );
            let mut layout_positions = self.positional.is_some();
            ui.toggle_value(&mut layout_positions, "auto-layout")
                .on_hover_text_at_pointer("(experimental) Organize nodes automatically.");
            if layout_positions {
                let positional =
                    self.positional.get_or_insert_with(Default::default);
                egui::ComboBox::from_label("Layout Method")
                    .selected_text(positional.as_str())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            positional,
                            PositionalLayout::force_directed(),
                            "Force Directed",
                        );
                    });
                positional.ui(ui);
            } else {
                self.positional = None;
            }
        });
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

    /// Set author for the node and all children.
    pub fn set_author(&mut self, author_id: u8) {
        self.author_id = author_id;
        for child in self.children.iter_mut() {
            child.set_author(author_id);
        }
    }

    /// Returns true if the node has no children.
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }

    /// Count the number of nodes in the tree including self.
    ///
    /// This is O(n) where n is the number of nodes, but n should be small
    pub fn count(&self) -> usize {
        1 + self.children.iter().map(Self::count).sum::<usize>()
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
        layout: Layout,
    ) -> Option<PathAction> {
        let active_path = active_path.unwrap_or(&[]);
        let mut ret = None; // the default, meaning no action is needed.

        // These are used for layout. We only calculate them if we need to.
        let mut global_centroid = Pos2::ZERO;
        let mut global_cum_mass = 0.0;
        #[allow(unused_variables)] // because pos is only used in debug builds,
        // but we still need the if block.
        if let Some(pos) = layout.positional {
            // Calculate the global centroid and mass of the tree.
            let (_, global_centroid_, global_cum_mass_) = self.centroid();
            global_centroid = global_centroid_;
            global_cum_mass = global_cum_mass_;

            // Debug build drawing.
            #[cfg(debug_assertions)]
            match pos {
                PositionalLayout::ForceDirected { gravity, .. } => {
                    egui::Area::new(egui::Id::new("global_centroid")).show(
                        ui.ctx(),
                        |ui| {
                            ui.painter().circle(
                                global_centroid,
                                gravity * global_cum_mass / 10.0,
                                egui::Color32::from_rgba_premultiplied(
                                    0, 128, 0, 128,
                                ),
                                egui::Stroke::NONE,
                            );
                        },
                    );
                }
            }
        }

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
            if let Some(action) = node.draw_one_node(
                ui,
                highlight_node,
                lock_topology,
                layout,
                global_centroid,
                global_cum_mass,
            ) {
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
                .add(egui::Button::image(egui::include_image!(
                    "../resources/add_node.png"
                )))
                .on_hover_text_at_pointer("Add an empty child node.");
            if add_child.clicked() {
                self.new_child_below();
            }
            let delete = ui
                .add(egui::Button::image(egui::include_image!(
                    "../resources/delete_subtree.png"
                )))
                .on_hover_text_at_pointer(
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
            let continue_ = ui
                .add(egui::Button::image(egui::include_image!(
                    "../resources/resume.png"
                )))
                .on_hover_text_at_pointer(
                    "Continue generating the current node.",
                );
            if continue_.clicked() {
                // Tell caller to continue generation on this node.
                *action = Some(Action {
                    continue_: true,
                    ..Default::default()
                });
            }
            let generate = ui
                .add(egui::Button::image(egui::include_image!(
                    "../resources/generate.png"
                )))
                .on_hover_text_at_pointer(
                    "Create a new node, select it, and continue generation.",
                );
            if generate.clicked() {
                // Tell caller to generate a new node.
                *action = Some(Action {
                    generate: Some(self.new_child_below()),
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

    /// Create a new child, below the parent's position.
    ///
    /// Returns the index of the new child.
    #[cfg(feature = "gui")]
    pub fn new_child_below(&mut self) -> usize {
        let mut child: Node<Meta> = Node::default();
        child.meta.pos = self
            .meta
            .rect()
            .expand(PADDING)
            .translate(egui::Vec2::new(0.0, self.meta.size.y + (PADDING * 2.0)))
            .center();
        self.add_child(child)
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
        layout: Layout,
        global_centroid: Pos2,
        global_cum_mass: f32,
    ) -> Option<Action> {
        // because this is only used in debug builds.
        #[allow(unused_assignments)]
        let mut repaint = false;
        let screen_rect = ui.ctx().screen_rect();
        if let Some(positional) = layout.positional {
            repaint = positional.apply(
                self,
                screen_rect,
                if cfg!(debug_assertions) {
                    Some(ui)
                } else {
                    None
                },
                global_centroid,
                global_cum_mass,
            );
            if repaint {
                // Positions have changed, request a repaint.
                ui.ctx().request_repaint();
            }
        }

        #[cfg(not(debug_assertions))]
        let frame = egui::Frame::window(&ui.ctx().style())
            .fill(egui::Color32::from_gray(64));

        #[cfg(debug_assertions)]
        let frame = egui::Frame::window(&ui.ctx().style())
            .stroke(if repaint {
                egui::Stroke::new(
                    self.meta.vel.abs().max_elem().min(PADDING).max(1.0),
                    egui::Color32::RED,
                )
            } else {
                egui::Stroke::NONE
            })
            .fill(egui::Color32::from_gray(64));

        let title = self
            .text
            .chars()
            .take(16)
            .chain(std::iter::once('…'))
            .collect::<String>();

        let window = egui::Window::new(&title)
            .id(egui::Id::new(self.meta.id))
            .collapsible(true)
            .title_bar(true)
            .default_open(!layout.auto_collapse || highlighted)
            .auto_sized()
            .default_pos(self.meta.pos)
            .current_pos(self.meta.pos)
            .frame(frame);

        let mut response = window.show(ui.ctx(), |ui| {
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

            if win.dragged() {
                // Otherwise the rounding done by egui will cause the nodes to
                // stand still because the velocity will be too small. We also
                // set it in the case the node has not been positioned yet.
                self.meta.pos = win.rect.min;
                self.meta.size = win.rect.size();
            }

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
        layout: Layout,
        mode: crate::story::DrawMode,
    ) -> Option<PathAction> {
        use crate::story::DrawMode;

        match mode {
            DrawMode::Nodes => {
                self.draw_nodes(ui, selected_path, lock_topology, layout)
            }
            DrawMode::Tree => {
                egui::ScrollArea::vertical()
                    .show(ui, |ui| {
                        self.draw_tree(
                            ui,
                            selected_path,
                            None, // current path (root is None)
                            0,    // depth
                            true, // selected
                            lock_topology,
                            layout,
                        )
                    })
                    .inner
            }
        }
    }

    /// Calculate (node_count, centroid, cumulative_mass) of the tree.
    /// Calculate (node_count, centroid, cumulative_mass) of the tree.
    pub fn centroid(&self) -> (usize, egui::Pos2, f32) {
        // Thank you ChatGPT 4o for pointing out that I was missing the mass
        // here. I was calculating the centroid, I wasn't taking the mass into
        // account. This is a weighted centroid calculation.
        let mut count = 1;
        let mut weighted_centroid_sum =
            self.meta.pos.to_vec2() * self.meta.mass();
        let mut mass = self.meta.mass();

        for child in self.children.iter() {
            let (c, child_centroid, child_mass) = child.centroid();
            count += c;
            weighted_centroid_sum += child_centroid.to_vec2() * child_mass;
            mass += child_mass;
        }

        let centroid = if mass > 0.0 {
            weighted_centroid_sum / mass
        } else {
            weighted_centroid_sum
        };

        (count, centroid.to_pos2(), mass)
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
        lock_topology: bool,
        layout: Layout,
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
            if layout.auto_collapse {
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
                        lock_topology,
                        layout,
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
