//! Artifact Caching for FastTeX
//!
//! This module handles caching of compilation artifacts including:
//! - .fmt preamble files
//! - .aux auxiliary files
//! - PDF fragments
//! - Final compiled PDFs

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use thiserror::Error;
use uuid::Uuid;

/// Error type for cache operations
#[derive(Error, Debug)]
pub enum CacheError {
    #[error("Artifact not found: {0}")]
    NotFound(String),
    
    #[error("Storage error: {0}")]
    StorageError(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Cache expired")]
    Expired,
}

/// Types of cacheable artifacts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArtifactType {
    /// Precompiled preamble format file
    FormatFile,
    /// Auxiliary file with cross-references
    AuxFile,
    /// PDF fragment from chapter compilation
    PdfFragment,
    /// Final merged PDF
    FinalPdf,
    /// TikZ externalized figure
    ExternalFigure,
    /// Bibliography database
    BibDatabase,
}

/// Metadata for a cached artifact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactMetadata {
    /// Unique artifact identifier
    pub id: Uuid,
    /// Project this artifact belongs to
    pub project_id: Uuid,
    /// Type of artifact
    pub artifact_type: ArtifactType,
    /// Hash of source content used to generate this artifact
    pub source_hash: String,
    /// Storage key/path
    pub storage_key: String,
    /// Size in bytes
    pub size_bytes: u64,
    /// When the artifact was created
    pub created_at: SystemTime,
    /// Time-to-live (how long to keep)
    pub ttl: Option<Duration>,
    /// Additional metadata
    pub extra: HashMap<String, String>,
}

impl ArtifactMetadata {
    /// Check if this artifact has expired
    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.ttl {
            if let Ok(elapsed) = self.created_at.elapsed() {
                return elapsed > ttl;
            }
        }
        false
    }
}

/// Cache key for looking up artifacts
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    /// Project identifier
    pub project_id: Uuid,
    /// Type of artifact
    pub artifact_type: ArtifactType,
    /// Hash of source content
    pub source_hash: String,
}

impl CacheKey {
    pub fn new(project_id: Uuid, artifact_type: ArtifactType, source_content: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(source_content);
        let hash = hex::encode(hasher.finalize());
        
        CacheKey {
            project_id,
            artifact_type,
            source_hash: hash,
        }
    }

    pub fn storage_key(&self) -> String {
        let type_prefix = match self.artifact_type {
            ArtifactType::FormatFile => "fmt",
            ArtifactType::AuxFile => "aux",
            ArtifactType::PdfFragment => "pdf-fragment",
            ArtifactType::FinalPdf => "pdf-final",
            ArtifactType::ExternalFigure => "figure",
            ArtifactType::BibDatabase => "bib",
        };
        
        format!("{}/{}/{}", type_prefix, self.project_id, self.source_hash)
    }
}

/// Configuration for the artifact cache
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Default TTL for format files (long-lived)
    pub fmt_ttl: Duration,
    /// Default TTL for aux files (session-based)
    pub aux_ttl: Duration,
    /// Default TTL for PDF fragments
    pub pdf_fragment_ttl: Duration,
    /// Maximum cache size in bytes
    pub max_size_bytes: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        CacheConfig {
            fmt_ttl: Duration::from_secs(7 * 24 * 60 * 60), // 7 days
            aux_ttl: Duration::from_secs(24 * 60 * 60),    // 1 day
            pdf_fragment_ttl: Duration::from_secs(60 * 60), // 1 hour
            max_size_bytes: 10 * 1024 * 1024 * 1024,       // 10 GB
        }
    }
}

/// Abstract storage backend trait
#[async_trait::async_trait]
pub trait StorageBackend: Send + Sync {
    /// Store data at the given key
    async fn put(&self, key: &str, data: &[u8]) -> Result<(), CacheError>;
    
    /// Retrieve data from the given key
    async fn get(&self, key: &str) -> Result<Vec<u8>, CacheError>;
    
    /// Delete data at the given key
    async fn delete(&self, key: &str) -> Result<(), CacheError>;
    
    /// Check if a key exists
    async fn exists(&self, key: &str) -> Result<bool, CacheError>;
    
    /// List all keys with a given prefix
    async fn list(&self, prefix: &str) -> Result<Vec<String>, CacheError>;
}

/// In-memory storage backend for testing
pub struct InMemoryStorage {
    data: tokio::sync::RwLock<HashMap<String, Vec<u8>>>,
}

impl InMemoryStorage {
    pub fn new() -> Self {
        InMemoryStorage {
            data: tokio::sync::RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl StorageBackend for InMemoryStorage {
    async fn put(&self, key: &str, data: &[u8]) -> Result<(), CacheError> {
        let mut store = self.data.write().await;
        store.insert(key.to_string(), data.to_vec());
        Ok(())
    }
    
    async fn get(&self, key: &str) -> Result<Vec<u8>, CacheError> {
        let store = self.data.read().await;
        store.get(key)
            .cloned()
            .ok_or_else(|| CacheError::NotFound(key.to_string()))
    }
    
    async fn delete(&self, key: &str) -> Result<(), CacheError> {
        let mut store = self.data.write().await;
        store.remove(key);
        Ok(())
    }
    
    async fn exists(&self, key: &str) -> Result<bool, CacheError> {
        let store = self.data.read().await;
        Ok(store.contains_key(key))
    }
    
    async fn list(&self, prefix: &str) -> Result<Vec<String>, CacheError> {
        let store = self.data.read().await;
        Ok(store.keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }
}

/// The main artifact cache
pub struct ArtifactCache<S: StorageBackend> {
    /// Storage backend
    storage: S,
    /// Configuration
    config: CacheConfig,
    /// Metadata store (would be Redis in production)
    metadata: tokio::sync::RwLock<HashMap<String, ArtifactMetadata>>,
}

impl<S: StorageBackend> ArtifactCache<S> {
    /// Create a new artifact cache with the given storage backend
    pub fn new(storage: S, config: CacheConfig) -> Self {
        ArtifactCache {
            storage,
            config,
            metadata: tokio::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Get the TTL for a given artifact type
    fn ttl_for_type(&self, artifact_type: ArtifactType) -> Duration {
        match artifact_type {
            ArtifactType::FormatFile => self.config.fmt_ttl,
            ArtifactType::AuxFile => self.config.aux_ttl,
            ArtifactType::PdfFragment => self.config.pdf_fragment_ttl,
            _ => self.config.pdf_fragment_ttl,
        }
    }

    /// Store an artifact in the cache
    pub async fn put(
        &self,
        key: &CacheKey,
        data: &[u8],
    ) -> Result<ArtifactMetadata, CacheError> {
        let storage_key = key.storage_key();
        
        // Store the data
        self.storage.put(&storage_key, data).await?;
        
        // Create and store metadata
        let metadata = ArtifactMetadata {
            id: Uuid::new_v4(),
            project_id: key.project_id,
            artifact_type: key.artifact_type,
            source_hash: key.source_hash.clone(),
            storage_key: storage_key.clone(),
            size_bytes: data.len() as u64,
            created_at: SystemTime::now(),
            ttl: Some(self.ttl_for_type(key.artifact_type)),
            extra: HashMap::new(),
        };
        
        {
            let mut meta_store = self.metadata.write().await;
            meta_store.insert(storage_key, metadata.clone());
        }
        
        Ok(metadata)
    }

    /// Get an artifact from the cache
    pub async fn get(&self, key: &CacheKey) -> Result<Vec<u8>, CacheError> {
        let storage_key = key.storage_key();
        
        // Check metadata for expiration
        {
            let meta_store = self.metadata.read().await;
            if let Some(metadata) = meta_store.get(&storage_key) {
                if metadata.is_expired() {
                    return Err(CacheError::Expired);
                }
            }
        }
        
        self.storage.get(&storage_key).await
    }

    /// Check if an artifact exists and is valid
    pub async fn exists(&self, key: &CacheKey) -> bool {
        let storage_key = key.storage_key();
        
        // Check metadata for expiration
        {
            let meta_store = self.metadata.read().await;
            if let Some(metadata) = meta_store.get(&storage_key) {
                if metadata.is_expired() {
                    return false;
                }
            }
        }
        
        self.storage.exists(&storage_key).await.unwrap_or(false)
    }

    /// Delete an artifact from the cache
    pub async fn delete(&self, key: &CacheKey) -> Result<(), CacheError> {
        let storage_key = key.storage_key();
        
        {
            let mut meta_store = self.metadata.write().await;
            meta_store.remove(&storage_key);
        }
        
        self.storage.delete(&storage_key).await
    }

    /// Invalidate all artifacts for a project
    pub async fn invalidate_project(&self, project_id: Uuid) -> Result<usize, CacheError> {
        let prefix = format!("{}", project_id);
        let keys = self.storage.list(&prefix).await?;
        
        let mut count = 0;
        for key in keys {
            self.storage.delete(&key).await?;
            count += 1;
        }
        
        {
            let mut meta_store = self.metadata.write().await;
            meta_store.retain(|_, v| v.project_id != project_id);
        }
        
        Ok(count)
    }

    /// Get metadata for an artifact
    pub async fn get_metadata(&self, key: &CacheKey) -> Option<ArtifactMetadata> {
        let storage_key = key.storage_key();
        let meta_store = self.metadata.read().await;
        meta_store.get(&storage_key).cloned()
    }
}

/// Helper for .fmt preamble caching
pub struct FmtCache<S: StorageBackend> {
    cache: ArtifactCache<S>,
}

impl<S: StorageBackend> FmtCache<S> {
    pub fn new(cache: ArtifactCache<S>) -> Self {
        FmtCache { cache }
    }

    /// Compute the cache key for a preamble
    pub fn preamble_key(project_id: Uuid, preamble_content: &[u8]) -> CacheKey {
        CacheKey::new(project_id, ArtifactType::FormatFile, preamble_content)
    }

    /// Check if a cached .fmt exists for this preamble
    pub async fn has_fmt(&self, project_id: Uuid, preamble_content: &[u8]) -> bool {
        let key = Self::preamble_key(project_id, preamble_content);
        self.cache.exists(&key).await
    }

    /// Get a cached .fmt file
    pub async fn get_fmt(&self, project_id: Uuid, preamble_content: &[u8]) -> Result<Vec<u8>, CacheError> {
        let key = Self::preamble_key(project_id, preamble_content);
        self.cache.get(&key).await
    }

    /// Store a compiled .fmt file
    pub async fn put_fmt(
        &self,
        project_id: Uuid,
        preamble_content: &[u8],
        fmt_data: &[u8],
    ) -> Result<ArtifactMetadata, CacheError> {
        let key = Self::preamble_key(project_id, preamble_content);
        self.cache.put(&key, fmt_data).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_put_get() {
        let storage = InMemoryStorage::new();
        let cache = ArtifactCache::new(storage, CacheConfig::default());
        
        let project_id = Uuid::new_v4();
        let content = b"test preamble content";
        let fmt_data = b"compiled format data";
        
        let key = CacheKey::new(project_id, ArtifactType::FormatFile, content);
        
        // Store
        let metadata = cache.put(&key, fmt_data).await.unwrap();
        assert_eq!(metadata.size_bytes, fmt_data.len() as u64);
        
        // Retrieve
        let retrieved = cache.get(&key).await.unwrap();
        assert_eq!(retrieved, fmt_data);
    }

    #[tokio::test]
    async fn test_cache_exists() {
        let storage = InMemoryStorage::new();
        let cache = ArtifactCache::new(storage, CacheConfig::default());
        
        let project_id = Uuid::new_v4();
        let key = CacheKey::new(project_id, ArtifactType::AuxFile, b"test");
        
        assert!(!cache.exists(&key).await);
        
        cache.put(&key, b"data").await.unwrap();
        
        assert!(cache.exists(&key).await);
    }

    #[tokio::test]
    async fn test_cache_delete() {
        let storage = InMemoryStorage::new();
        let cache = ArtifactCache::new(storage, CacheConfig::default());
        
        let project_id = Uuid::new_v4();
        let key = CacheKey::new(project_id, ArtifactType::PdfFragment, b"test");
        
        cache.put(&key, b"data").await.unwrap();
        assert!(cache.exists(&key).await);
        
        cache.delete(&key).await.unwrap();
        assert!(!cache.exists(&key).await);
    }

    #[tokio::test]
    async fn test_fmt_cache() {
        let storage = InMemoryStorage::new();
        let cache = ArtifactCache::new(storage, CacheConfig::default());
        let fmt_cache = FmtCache::new(cache);
        
        let project_id = Uuid::new_v4();
        let preamble = b"\\documentclass{article}\n\\usepackage{amsmath}";
        let fmt_data = b"compiled format file data";
        
        // Initially not cached
        assert!(!fmt_cache.has_fmt(project_id, preamble).await);
        
        // Store
        fmt_cache.put_fmt(project_id, preamble, fmt_data).await.unwrap();
        
        // Now cached
        assert!(fmt_cache.has_fmt(project_id, preamble).await);
        
        // Retrieve
        let retrieved = fmt_cache.get_fmt(project_id, preamble).await.unwrap();
        assert_eq!(retrieved, fmt_data);
        
        // Different preamble should not match
        let different_preamble = b"\\documentclass{book}";
        assert!(!fmt_cache.has_fmt(project_id, different_preamble).await);
    }

    #[test]
    fn test_cache_key_storage_key() {
        let project_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let key = CacheKey::new(project_id, ArtifactType::FormatFile, b"test");
        
        let storage_key = key.storage_key();
        assert!(storage_key.starts_with("fmt/"));
        assert!(storage_key.contains(&project_id.to_string()));
    }

    #[test]
    fn test_artifact_metadata_expiration() {
        let mut metadata = ArtifactMetadata {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            artifact_type: ArtifactType::AuxFile,
            source_hash: "abc123".to_string(),
            storage_key: "test/key".to_string(),
            size_bytes: 100,
            created_at: SystemTime::now(),
            ttl: Some(Duration::from_secs(0)), // Immediate expiration for testing
            extra: HashMap::new(),
        };
        
        // Should be expired immediately with 0 TTL
        std::thread::sleep(Duration::from_millis(10));
        assert!(metadata.is_expired());
        
        // No TTL means never expires
        metadata.ttl = None;
        assert!(!metadata.is_expired());
    }
}
