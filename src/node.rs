use serde::{Deserialize, Serialize};

/// Represents a detokenized token and its author.
#[derive(Serialize, Deserialize)]
pub struct Piece {
    author_id: u8,
    text: String,
}

/// Node data. Contains a paragraph within a story tree.
#[derive(Default, Serialize, Deserialize)]
pub struct Node {
    /// The text of the paragraph.
    pub pieces: Vec<Piece>,
    /// The children of this node.
    pub children: Vec<Node>,
}

impl Node {
    /// Adds a child to self. Returns the index of the child.
    pub fn add_child(&mut self, child: Node) -> usize {
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
    pub fn extend_strings<I, S>(&mut self, author_id: u8, strings: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.pieces.extend(strings.into_iter().map(|text| Piece {
            author_id,
            text: text.into(),
        }));
    }

    /// Iterate nodes over a path, including self.
    ///
    /// # Panics
    /// - If the path is invalid.
    pub fn iter_path_nodes<'a>(&'a self, path: &'a [usize]) -> impl Iterator<Item = &'a Node> + 'a {
        let mut node = self;
        std::iter::once(node).chain(path.iter().map(move |&i| {
            node = &node.children[i];
            node
        }))
    }

    /// Iterate Pieces of the node.
    pub fn iter_pieces<'a>(&'a self) -> impl Iterator<Item = &'a Piece> + 'a {
        self.pieces.iter()
    }

    /// Iterate text over this node.
    pub fn iter_text<'a>(&'a self) -> impl Iterator<Item = &str> + 'a {
        self.iter_pieces().map(|piece| piece.text.as_str())
    }

    /// Iterate text over a path, including self, joining each node with a separator.
    ///
    /// # Panics
    /// - If the path is invalid.
    pub fn iter_path_text<'a>(
        &'a self,
        path: &'a [usize],
        separator: &'a str,
    ) -> impl Iterator<Item = &str> + 'a {
        self.iter_path_nodes(path)
            .map(move |node| std::iter::once(separator).chain(node.iter_text()))
            .flatten()
            .skip(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iter_path_nodes() {
        let mut root = Node::default();
        root.extend_strings(0, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(0, root.add_child(Node::default()));
        root.children[0].extend_strings(1, vec!["c".to_string(), "d".to_string()]);
        assert_eq!(0, root.children[0].add_child(Node::default()));
        root.children[0].children[0].extend_strings(2, vec!["e".to_string(), "f".to_string()]);

        let path = [0, 0];
        let nodes: Vec<_> = root.iter_path_nodes(&path).collect();
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0].pieces[0].text, "a");
        assert_eq!(nodes[0].pieces[1].text, "b");
        assert_eq!(nodes[1].pieces[0].text, "c");
        assert_eq!(nodes[1].pieces[1].text, "d");
        assert_eq!(nodes[2].pieces[0].text, "e");
        assert_eq!(nodes[2].pieces[1].text, "f");
    }

    #[test]
    fn iter_path_text() {
        let mut root = Node::default();
        root.extend_strings(0, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(0, root.add_child(Node::default()));
        root.children[0].extend_strings(1, vec!["c".to_string(), "d".to_string()]);
        assert_eq!(0, root.children[0].add_child(Node::default()));
        root.children[0].children[0].extend_strings(2, vec!["e".to_string(), "f".to_string()]);

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
}
