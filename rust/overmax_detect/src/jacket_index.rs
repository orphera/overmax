/// 자켓 매칭 결과. `image_id`는 자켓 인덱스의 이미지 ID (곡 ID 문자열).
#[derive(Clone, Debug)]
pub struct JacketHit {
    pub image_id: String,
    pub similarity: f32,
}

/// `DetectionPipeline`이 자켓 인덱스에 요구하는 기능.
///
/// 감지 코드가 저장소 구현(`overmax_data`)에 직접 의존하지 않도록 경계를 둔다.
/// 매 프레임 경로에서 정적 디스패치로 호출된다.
pub trait JacketIndex {
    /// 인덱스에 자켓 데이터가 적재되어 매칭이 가능한지.
    fn is_ready(&self) -> bool;

    /// 매칭 성공으로 볼 최소 유사도.
    fn similarity_threshold(&self) -> f32;

    /// 자켓 ROI가 자켓 DB 대표 분포 근처인지 보는 사전 게이트.
    fn check_centroid_kernel(
        &self,
        data: &[u8],
        width: usize,
        height: usize,
        channels: usize,
    ) -> bool;

    /// 가장 유사한 자켓을 찾는다.
    fn match_jacket(
        &self,
        data: &[u8],
        width: usize,
        height: usize,
        channels: usize,
    ) -> Option<JacketHit>;
}
