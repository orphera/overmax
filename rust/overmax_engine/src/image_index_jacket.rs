//! `overmax_data`의 이미지 인덱스(`ImageIndexDb` + `JacketMatcher`)를
//! 감지 쪽 `JacketIndex` 경계에 연결하는 어댑터.

use overmax_data::{ImageIndexDb, JacketMatcher};
use overmax_detect::{JacketHit, JacketIndex};

pub struct ImageIndexJacket {
    db: ImageIndexDb,
    matcher: JacketMatcher,
}

impl ImageIndexJacket {
    pub fn new(db: ImageIndexDb) -> Self {
        let matcher = db.matcher();
        Self { db, matcher }
    }

    pub fn db_mut(&mut self) -> &mut ImageIndexDb {
        &mut self.db
    }
}

impl JacketIndex for ImageIndexJacket {
    #[inline]
    fn is_ready(&self) -> bool {
        self.db.is_ready()
    }

    #[inline]
    fn similarity_threshold(&self) -> f32 {
        self.matcher.similarity_threshold()
    }

    #[inline]
    fn check_centroid_kernel(
        &self,
        data: &[u8],
        width: usize,
        height: usize,
        channels: usize,
    ) -> bool {
        self.matcher
            .check_centroid_kernel(data, width, height, channels)
    }

    #[inline]
    fn match_jacket(
        &self,
        data: &[u8],
        width: usize,
        height: usize,
        channels: usize,
    ) -> Option<JacketHit> {
        self.matcher
            .match_jacket(data, width, height, channels)
            .map(|m| JacketHit {
                image_id: m.image_id,
                similarity: m.similarity,
            })
    }
}

impl From<ImageIndexDb> for ImageIndexJacket {
    fn from(db: ImageIndexDb) -> Self {
        Self::new(db)
    }
}
