//! Dependency Graph for LaTeX Document Compilation
//!
//! This module provides the core data structures and algorithms for building
//! and analyzing the dependency graph of a LaTeX document. The graph represents
//! the relationships between document components (preamble, chapters, figures)
//! and enables parallel compilation scheduling.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::algo::toposort;
use petgraph::Direction;
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use thiserror::Error;
use uuid::Uuid;

/// Unique identifier for a node in the dependency graph
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub Uuid);

impl NodeId {
    pub fn new() -> Self {
        NodeId(Uuid::new_v4())
    }
}

impl Default for NodeId {
    fn default() -> Self {
        Self::new()
    }
}

/// Content hash for change detection
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContentHash(pub String);

impl ContentHash {
    /// Compute hash from content bytes
    pub fn from_content(content: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(content);
        let result = hasher.finalize();
        ContentHash(hex::encode(result))
    }
}

/// Type of compilation node
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeType {
    /// Document preamble (packages, macros) - compiled to .fmt
    Preamble,
    /// Main document entry point
    MainDocument,
    /// Included chapter (\include{})
    Chapter,
    /// External figure (tikzexternalize, standalone)
    Figure,
    /// Bibliography file
    Bibliography,
    /// Style file (.sty)
    StyleFile,
    /// Class file (.cls)
    ClassFile,
}

/// A node in the compilation dependency graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileNode {
    /// Unique identifier
    pub id: NodeId,
    /// Type of this node
    pub node_type: NodeType,
    /// Path to source file relative to project root
    pub source_path: PathBuf,
    /// Current content hash
    pub content_hash: ContentHash,
    /// Hash at last successful compilation
    pub last_compiled_hash: Option<ContentHash>,
    /// Human-readable name for this node
    pub display_name: String,
    /// Whether this node requires .fmt preamble
    pub requires_fmt: bool,
    /// Additional files needed for compilation
    pub auxiliary_files: Vec<PathBuf>,
}

impl CompileNode {
    /// Create a new compile node
    pub fn new(
        node_type: NodeType,
        source_path: PathBuf,
        content: &[u8],
    ) -> Self {
        let display_name = source_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let requires_fmt = matches!(
            node_type,
            NodeType::MainDocument | NodeType::Chapter | NodeType::Figure
        );

        CompileNode {
            id: NodeId::new(),
            node_type,
            source_path,
            content_hash: ContentHash::from_content(content),
            last_compiled_hash: None,
            display_name,
            requires_fmt,
            auxiliary_files: Vec::new(),
        }
    }

    /// Check if this node needs recompilation
    pub fn needs_recompile(&self) -> bool {
        match &self.last_compiled_hash {
            Some(hash) => hash != &self.content_hash,
            None => true,
        }
    }

    /// Mark as successfully compiled
    pub fn mark_compiled(&mut self) {
        self.last_compiled_hash = Some(self.content_hash.clone());
    }
}

/// Error type for dependency graph operations
#[derive(Error, Debug)]
pub enum GraphError {
    #[error("Node not found: {0:?}")]
    NodeNotFound(NodeId),
    
    #[error("Cycle detected in dependency graph")]
    CycleDetected,
    
    #[error("Invalid graph structure: {0}")]
    InvalidStructure(String),
    
    #[error("Parse error: {0}")]
    ParseError(String),
}

/// The dependency graph for a LaTeX project
/// 
/// This graph represents the compilation dependencies between document components.
/// It is a directed acyclic graph (DAG) where edges represent "depends on" relationships.
#[derive(Debug)]
pub struct DependencyGraph {
    /// The underlying directed graph
    graph: DiGraph<CompileNode, ()>,
    /// Map from NodeId to graph index
    node_indices: HashMap<NodeId, NodeIndex>,
    /// The root node (main document or preamble)
    root: Option<NodeId>,
    /// Project root directory
    project_root: PathBuf,
}

impl DependencyGraph {
    /// Create a new empty dependency graph
    pub fn new(project_root: PathBuf) -> Self {
        DependencyGraph {
            graph: DiGraph::new(),
            node_indices: HashMap::new(),
            root: None,
            project_root,
        }
    }

    /// Add a node to the graph
    pub fn add_node(&mut self, node: CompileNode) -> NodeId {
        let id = node.id;
        let index = self.graph.add_node(node);
        self.node_indices.insert(id, index);
        id
    }

    /// Add a dependency edge: `from` depends on `to`
    pub fn add_dependency(&mut self, from: NodeId, to: NodeId) -> Result<(), GraphError> {
        let from_idx = self.node_indices.get(&from)
            .ok_or(GraphError::NodeNotFound(from))?;
        let to_idx = self.node_indices.get(&to)
            .ok_or(GraphError::NodeNotFound(to))?;
        
        self.graph.add_edge(*from_idx, *to_idx, ());
        Ok(())
    }

    /// Set the root node of the graph
    pub fn set_root(&mut self, root: NodeId) -> Result<(), GraphError> {
        if !self.node_indices.contains_key(&root) {
            return Err(GraphError::NodeNotFound(root));
        }
        self.root = Some(root);
        Ok(())
    }

    /// Get a node by ID
    pub fn get_node(&self, id: NodeId) -> Option<&CompileNode> {
        self.node_indices.get(&id)
            .and_then(|idx| self.graph.node_weight(*idx))
    }

    /// Get a mutable reference to a node by ID
    pub fn get_node_mut(&mut self, id: NodeId) -> Option<&mut CompileNode> {
        self.node_indices.get(&id)
            .and_then(|idx| self.graph.node_weight_mut(*idx))
    }

    /// Get all nodes in the graph
    pub fn nodes(&self) -> impl Iterator<Item = &CompileNode> {
        self.graph.node_weights()
    }

    /// Get the root node
    pub fn root(&self) -> Option<&CompileNode> {
        self.root.and_then(|id| self.get_node(id))
    }

    /// Get direct dependencies of a node (nodes it depends on)
    pub fn dependencies(&self, id: NodeId) -> Result<Vec<NodeId>, GraphError> {
        let idx = self.node_indices.get(&id)
            .ok_or(GraphError::NodeNotFound(id))?;
        
        Ok(self.graph
            .neighbors_directed(*idx, Direction::Outgoing)
            .filter_map(|neighbor_idx| {
                self.graph.node_weight(neighbor_idx).map(|n| n.id)
            })
            .collect())
    }

    /// Get nodes that depend on this node (dependents)
    pub fn dependents(&self, id: NodeId) -> Result<Vec<NodeId>, GraphError> {
        let idx = self.node_indices.get(&id)
            .ok_or(GraphError::NodeNotFound(id))?;
        
        Ok(self.graph
            .neighbors_directed(*idx, Direction::Incoming)
            .filter_map(|neighbor_idx| {
                self.graph.node_weight(neighbor_idx).map(|n| n.id)
            })
            .collect())
    }

    /// Perform topological sort of the graph
    /// Returns nodes in order such that dependencies come before dependents
    pub fn topological_order(&self) -> Result<Vec<NodeId>, GraphError> {
        match toposort(&self.graph, None) {
            Ok(indices) => {
                // Reverse because toposort gives dependents first
                Ok(indices.into_iter()
                    .rev()
                    .filter_map(|idx| self.graph.node_weight(idx).map(|n| n.id))
                    .collect())
            }
            Err(_) => Err(GraphError::CycleDetected),
        }
    }

    /// Find all nodes that need recompilation
    /// This includes nodes with changed content AND their dependents
    pub fn nodes_needing_recompile(&self) -> Result<HashSet<NodeId>, GraphError> {
        let mut needs_recompile = HashSet::new();
        
        // First pass: find directly changed nodes
        for node in self.graph.node_weights() {
            if node.needs_recompile() {
                needs_recompile.insert(node.id);
            }
        }
        
        // Second pass: propagate to dependents using BFS
        let mut queue: VecDeque<NodeId> = needs_recompile.iter().copied().collect();
        
        while let Some(id) = queue.pop_front() {
            for dependent_id in self.dependents(id)? {
                if !needs_recompile.contains(&dependent_id) {
                    needs_recompile.insert(dependent_id);
                    queue.push_back(dependent_id);
                }
            }
        }
        
        Ok(needs_recompile)
    }

    /// Find nodes that can be compiled in parallel (same topological level)
    /// Returns groups of node IDs where all nodes in a group can run concurrently
    pub fn parallel_groups(&self) -> Result<Vec<Vec<NodeId>>, GraphError> {
        let topo_order = self.topological_order()?;
        let needs_recompile = self.nodes_needing_recompile()?;
        
        // Filter to only nodes needing recompile
        let filtered: Vec<NodeId> = topo_order
            .into_iter()
            .filter(|id| needs_recompile.contains(id))
            .collect();
        
        if filtered.is_empty() {
            return Ok(Vec::new());
        }
        
        // Calculate the level (longest path from any root) for each node
        let mut levels: HashMap<NodeId, usize> = HashMap::new();
        
        for &id in &filtered {
            let deps = self.dependencies(id)?;
            let max_dep_level = deps
                .iter()
                .filter_map(|dep_id| levels.get(dep_id))
                .max()
                .copied()
                .unwrap_or(0);
            
            let level = if deps.is_empty() { 0 } else { max_dep_level + 1 };
            levels.insert(id, level);
        }
        
        // Group by level
        let max_level = levels.values().max().copied().unwrap_or(0);
        let mut groups: Vec<Vec<NodeId>> = vec![Vec::new(); max_level + 1];
        
        for (id, level) in levels {
            groups[level].push(id);
        }
        
        // Remove empty groups
        groups.retain(|g| !g.is_empty());
        
        Ok(groups)
    }

    /// Get the number of nodes in the graph
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Get the project root directory
    pub fn project_root(&self) -> &PathBuf {
        &self.project_root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_node(name: &str, node_type: NodeType) -> CompileNode {
        CompileNode::new(
            node_type,
            PathBuf::from(format!("{}.tex", name)),
            name.as_bytes(),
        )
    }

    #[test]
    fn test_empty_graph() {
        let graph = DependencyGraph::new(PathBuf::from("/project"));
        assert_eq!(graph.node_count(), 0);
        assert!(graph.root().is_none());
    }

    #[test]
    fn test_add_node() {
        let mut graph = DependencyGraph::new(PathBuf::from("/project"));
        let node = create_test_node("main", NodeType::MainDocument);
        let id = graph.add_node(node);
        
        assert_eq!(graph.node_count(), 1);
        assert!(graph.get_node(id).is_some());
    }

    #[test]
    fn test_dependencies() {
        let mut graph = DependencyGraph::new(PathBuf::from("/project"));
        
        let preamble = create_test_node("preamble", NodeType::Preamble);
        let main = create_test_node("main", NodeType::MainDocument);
        let chapter1 = create_test_node("chapter1", NodeType::Chapter);
        let chapter2 = create_test_node("chapter2", NodeType::Chapter);
        
        let preamble_id = graph.add_node(preamble);
        let main_id = graph.add_node(main);
        let chapter1_id = graph.add_node(chapter1);
        let chapter2_id = graph.add_node(chapter2);
        
        // main depends on preamble
        graph.add_dependency(main_id, preamble_id).unwrap();
        // chapters depend on preamble
        graph.add_dependency(chapter1_id, preamble_id).unwrap();
        graph.add_dependency(chapter2_id, preamble_id).unwrap();
        // main depends on chapters
        graph.add_dependency(main_id, chapter1_id).unwrap();
        graph.add_dependency(main_id, chapter2_id).unwrap();
        
        graph.set_root(main_id).unwrap();
        
        // Check dependencies
        let main_deps = graph.dependencies(main_id).unwrap();
        assert_eq!(main_deps.len(), 3); // preamble, chapter1, chapter2
        
        // Check dependents
        let preamble_dependents = graph.dependents(preamble_id).unwrap();
        assert_eq!(preamble_dependents.len(), 3); // main, chapter1, chapter2
    }

    #[test]
    fn test_topological_order() {
        let mut graph = DependencyGraph::new(PathBuf::from("/project"));
        
        let preamble = create_test_node("preamble", NodeType::Preamble);
        let chapter = create_test_node("chapter", NodeType::Chapter);
        let main = create_test_node("main", NodeType::MainDocument);
        
        let preamble_id = graph.add_node(preamble);
        let chapter_id = graph.add_node(chapter);
        let main_id = graph.add_node(main);
        
        graph.add_dependency(chapter_id, preamble_id).unwrap();
        graph.add_dependency(main_id, chapter_id).unwrap();
        
        let order = graph.topological_order().unwrap();
        
        // Preamble should come first, then chapter, then main
        let preamble_pos = order.iter().position(|&id| id == preamble_id).unwrap();
        let chapter_pos = order.iter().position(|&id| id == chapter_id).unwrap();
        let main_pos = order.iter().position(|&id| id == main_id).unwrap();
        
        assert!(preamble_pos < chapter_pos);
        assert!(chapter_pos < main_pos);
    }

    #[test]
    fn test_parallel_groups() {
        let mut graph = DependencyGraph::new(PathBuf::from("/project"));
        
        let preamble = create_test_node("preamble", NodeType::Preamble);
        let chapter1 = create_test_node("chapter1", NodeType::Chapter);
        let chapter2 = create_test_node("chapter2", NodeType::Chapter);
        let chapter3 = create_test_node("chapter3", NodeType::Chapter);
        
        let preamble_id = graph.add_node(preamble);
        let chapter1_id = graph.add_node(chapter1);
        let chapter2_id = graph.add_node(chapter2);
        let chapter3_id = graph.add_node(chapter3);
        
        // All chapters depend on preamble
        graph.add_dependency(chapter1_id, preamble_id).unwrap();
        graph.add_dependency(chapter2_id, preamble_id).unwrap();
        graph.add_dependency(chapter3_id, preamble_id).unwrap();
        
        let groups = graph.parallel_groups().unwrap();
        
        // Should have 2 groups: preamble first, then all chapters in parallel
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].len(), 1); // preamble
        assert_eq!(groups[1].len(), 3); // all chapters
    }

    #[test]
    fn test_needs_recompile() {
        let mut node = create_test_node("test", NodeType::Chapter);
        
        // New node should need recompile
        assert!(node.needs_recompile());
        
        // After marking compiled, should not need recompile
        node.mark_compiled();
        assert!(!node.needs_recompile());
        
        // After content change, should need recompile
        node.content_hash = ContentHash::from_content(b"changed content");
        assert!(node.needs_recompile());
    }

    #[test]
    fn test_content_hash() {
        let hash1 = ContentHash::from_content(b"hello");
        let hash2 = ContentHash::from_content(b"hello");
        let hash3 = ContentHash::from_content(b"world");
        
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }
}
