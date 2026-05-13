use rusqlite::types::ValueRef;
use rusqlite::{Connection, Result};
use std::path::{Path, PathBuf};

const DEFAULT_TOP_K: usize = 10;
const HOG_LEN: usize = 1764;

#[derive(Clone, Debug, PartialEq)]
pub struct ImageMatch {
    pub image_id: String,
    pub similarity: f32,
}

#[derive(Clone, Debug)]
struct ImageEntry {
    image_id: String,
    phash: u64,
    dhash: u64,
    ahash: u64,
    hog: Vec<f32>,
    hog_norm: f32,
}

#[derive(Clone, Debug)]
pub struct ImageIndexDb {
    db_path: PathBuf,
    similarity_threshold: f32,
    entries: Vec<ImageEntry>,
}

impl ImageIndexDb {
    pub fn new(db_path: impl AsRef<Path>, similarity_threshold: f32) -> Self {
        Self {
            db_path: db_path.as_ref().to_path_buf(),
            similarity_threshold,
            entries: Vec::new(),
        }
    }

    pub fn load(&mut self) -> Result<usize> {
        let conn = Connection::open(&self.db_path)?;
        self.entries = load_entries(&conn)?;
        Ok(self.entries.len())
    }

    pub fn song_count(&self) -> usize {
        self.entries.len()
    }

    pub fn is_ready(&self) -> bool {
        !self.entries.is_empty()
    }

    pub fn search(
        &self,
        data: &[u8],
        width: usize,
        height: usize,
        channels: usize,
    ) -> Option<ImageMatch> {
        self.search_with_top_k(data, width, height, channels, DEFAULT_TOP_K)
    }

    pub fn search_with_top_k(
        &self,
        data: &[u8],
        width: usize,
        height: usize,
        channels: usize,
        top_k: usize,
    ) -> Option<ImageMatch> {
        if self.entries.is_empty() || top_k == 0 {
            return None;
        }
        let query = QueryFeatures::from_image(data, width, height, channels)?;
        self.best_match(&query, top_k)
    }

    fn best_match(&self, query: &QueryFeatures, top_k: usize) -> Option<ImageMatch> {
        let mut candidates = self.hash_candidates(query, top_k);
        candidates.sort_by(|a, b| b.1.total_cmp(&a.1));
        candidates.into_iter().next().and_then(|(idx, similarity)| {
            (similarity >= self.similarity_threshold).then(|| ImageMatch {
                image_id: self.entries[idx].image_id.clone(),
                similarity,
            })
        })
    }

    fn hash_candidates(&self, query: &QueryFeatures, top_k: usize) -> Vec<(usize, f32)> {
        let mut hashes = self
            .entries
            .iter()
            .enumerate()
            .map(|(idx, entry)| (idx, hash_score(entry, query)))
            .collect::<Vec<_>>();
        hashes.sort_by(|a, b| a.1.total_cmp(&b.1));
        hashes
            .into_iter()
            .take(top_k.min(self.entries.len()))
            .map(|(idx, score)| (idx, similarity(&self.entries[idx], query, score)))
            .collect()
    }
}

struct QueryFeatures {
    phash: u64,
    dhash: u64,
    ahash: u64,
    hog: Vec<f32>,
    hog_norm: f32,
}

impl QueryFeatures {
    fn from_image(data: &[u8], width: usize, height: usize, channels: usize) -> Option<Self> {
        let (phash, dhash, ahash, hog) =
            overmax_cv::compute_image_features(data, width, height, channels).ok()?;
        let hog_norm = vector_norm(&hog).max(1.0);
        Some(Self {
            phash: parse_hash(&phash)?,
            dhash: parse_hash(&dhash)?,
            ahash: parse_hash(&ahash)?,
            hog,
            hog_norm,
        })
    }
}

fn load_entries(conn: &Connection) -> Result<Vec<ImageEntry>> {
    let mut stmt = conn.prepare(
        "SELECT image_id, phash, dhash, ahash, hog
         FROM images
         WHERE id IN (SELECT MAX(id) FROM images GROUP BY image_id)
         ORDER BY id ASC",
    )?;
    let rows = stmt.query_map([], row_to_entry)?;
    Ok(rows.filter_map(Result::ok).collect())
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> Result<ImageEntry> {
    let image_id = value_to_string(row.get_ref(0)?)?;
    let phash: String = row.get(1)?;
    let dhash: String = row.get(2)?;
    let ahash: String = row.get(3)?;
    let hog_blob: Vec<u8> = row.get(4)?;
    parse_entry(image_id, &phash, &dhash, &ahash, &hog_blob)
        .ok_or_else(|| rusqlite::Error::InvalidQuery)
}

fn value_to_string(value: ValueRef<'_>) -> Result<String> {
    match value {
        ValueRef::Integer(value) => Ok(value.to_string()),
        ValueRef::Text(value) => Ok(String::from_utf8_lossy(value).to_string()),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

fn parse_entry(
    image_id: String,
    phash: &str,
    dhash: &str,
    ahash: &str,
    hog_blob: &[u8],
) -> Option<ImageEntry> {
    let hog = parse_hog_blob(hog_blob)?;
    let hog_norm = vector_norm(&hog).max(1.0);
    Some(ImageEntry {
        image_id,
        phash: parse_hash(phash)?,
        dhash: parse_hash(dhash)?,
        ahash: parse_hash(ahash)?,
        hog,
        hog_norm,
    })
}

fn parse_hog_blob(blob: &[u8]) -> Option<Vec<f32>> {
    if blob.len() != HOG_LEN * std::mem::size_of::<f32>() {
        return None;
    }
    let mut values = Vec::with_capacity(HOG_LEN);
    for chunk in blob.chunks_exact(4) {
        values.push(f32::from_le_bytes(chunk.try_into().ok()?));
    }
    Some(values)
}

fn parse_hash(value: &str) -> Option<u64> {
    u64::from_str_radix(value, 16).ok()
}

fn hash_score(entry: &ImageEntry, query: &QueryFeatures) -> f32 {
    0.5 * (entry.phash ^ query.phash).count_ones() as f32
        + 0.3 * (entry.dhash ^ query.dhash).count_ones() as f32
        + 0.2 * (entry.ahash ^ query.ahash).count_ones() as f32
}

fn similarity(entry: &ImageEntry, query: &QueryFeatures, hash_score: f32) -> f32 {
    let hash_sim = (1.0 - hash_score / 64.0).clamp(0.0, 1.0);
    let hog_sim = dot(&entry.hog, &query.hog) / (entry.hog_norm * query.hog_norm);
    0.45 * hash_sim + 0.55 * hog_sim
}

fn dot(left: &[f32], right: &[f32]) -> f32 {
    left.iter().zip(right).map(|(a, b)| a * b).sum()
}

fn vector_norm(values: &[f32]) -> f32 {
    values.iter().map(|value| value * value).sum::<f32>().sqrt()
}

#[cfg(test)]
mod tests {
    use super::ImageIndexDb;
    use rusqlite::{params, Connection};
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn loads_latest_distinct_image_rows() {
        let db_path = make_db("duplicates");
        let conn = create_images_db(&db_path);
        insert_image(&conn, "song-a", "1", 0.5);
        insert_image(&conn, "song-a", "2", 0.6);
        insert_image(&conn, "song-b", "3", 0.7);

        let mut db = ImageIndexDb::new(&db_path, 0.7);
        assert_eq!(db.load().unwrap(), 2);
        assert_eq!(db.entries[0].image_id, "song-a");
        assert_eq!(db.entries[0].phash, 2);
        assert_eq!(db.entries[1].image_id, "song-b");
    }

    #[test]
    fn skips_invalid_hog_rows() {
        let db_path = make_db("invalid-hog");
        let conn = create_images_db(&db_path);
        conn.execute(
            "INSERT INTO images (image_id, phash, dhash, ahash, hog) VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["bad", "1", "1", "1", vec![0u8; 12]],
        )
        .unwrap();
        insert_image(&conn, "good", "2", 0.5);

        let mut db = ImageIndexDb::new(&db_path, 0.7);
        assert_eq!(db.load().unwrap(), 1);
        assert_eq!(db.entries[0].image_id, "good");
    }

    #[test]
    fn empty_db_returns_no_match() {
        let db_path = make_db("empty");
        create_images_db(&db_path);

        let mut db = ImageIndexDb::new(&db_path, 0.7);
        assert_eq!(db.load().unwrap(), 0);
        assert!(!db.is_ready());
        assert!(db.search(&[0; 64], 8, 8, 1).is_none());
    }

    #[test]
    fn searches_cached_entries_without_db_io() {
        let db_path = make_db("search");
        let conn = create_images_db(&db_path);
        let image = gradient_image();
        let (phash, dhash, ahash, hog) =
            overmax_cv::compute_image_features(&image, 8, 8, 1).unwrap();
        insert_image_with_features(&conn, "target", &phash, &dhash, &ahash, &hog);
        insert_image(&conn, "other", "ffffffffffffffff", 0.1);
        drop(conn);

        let mut db = ImageIndexDb::new(&db_path, 0.7);
        db.load().unwrap();
        fs::remove_file(&db_path).unwrap();

        let found = db.search(&image, 8, 8, 1).unwrap();
        assert_eq!(found.image_id, "target");
        assert!(found.similarity >= 0.99);
    }

    fn make_db(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("overmax-image-index-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.join("image_index.db")
    }

    fn create_images_db(path: &PathBuf) -> Connection {
        let conn = Connection::open(path).unwrap();
        conn.execute(
            "CREATE TABLE images (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                image_id TEXT NOT NULL,
                phash TEXT NOT NULL,
                dhash TEXT NOT NULL,
                ahash TEXT NOT NULL,
                hog BLOB NOT NULL,
                orb BLOB
            )",
            [],
        )
        .unwrap();
        conn
    }

    fn insert_image(conn: &Connection, image_id: &str, hash: &str, hog_value: f32) {
        let hog = vec![hog_value; 1764];
        insert_image_with_hog(conn, image_id, hash, &hog);
    }

    fn insert_image_with_hog(conn: &Connection, image_id: &str, hash: &str, hog: &[f32]) {
        insert_image_with_features(conn, image_id, hash, hash, hash, hog);
    }

    fn insert_image_with_features(
        conn: &Connection,
        image_id: &str,
        phash: &str,
        dhash: &str,
        ahash: &str,
        hog: &[f32],
    ) {
        conn.execute(
            "INSERT INTO images (image_id, phash, dhash, ahash, hog) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![image_id, phash, dhash, ahash, hog_blob(hog)],
        )
        .unwrap();
    }

    fn hog_blob(values: &[f32]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }

    fn gradient_image() -> Vec<u8> {
        (0..64).map(|idx| (idx * 4) as u8).collect()
    }
}
