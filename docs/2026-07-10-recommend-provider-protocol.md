# Recommend Provider Protocol (v1)

Overmax는 지금 로컬 floor 기반 추천(`overmax_data::service::recommend::Recommender`)만 제공한다.
이 문서는 외부 커뮤니티 서비스(행이봇/로페봇/디맥지지 등)가 자체 추천 로직을 HTTP 엔드포인트로
노출하면 Overmax가 그 결과를 오버레이에 표시할 수 있도록 하는 **얇은 프로토콜**과, Overmax 측
구현 방향을 정의한다.

Overmax는 이 프로토콜의 **viewer(소비자)**로만 동작한다. 추천 로직 자체는 만들지 않으며, provider가
없거나 응답하지 않아도 기존 로컬 `Recommender`가 항상 baseline으로 계속 동작해야 한다.

---

## 1. 문제 정의

### 현재 상황

| 역할 | Overmax (현재) | 커뮤니티 봇 (행이봇, 로페봇, 디맥지지 등) |
|------|----------------|------------------------------------------|
| 화면 인식 / alt-tab 없음 | ✅ | ❌ (별도 창·탭 전환 필요) |
| 추천 알고리즘 다양성 | ❌ (floor 기반 1종) | ✅ (개인화·미션·트렌드 등) |
| 공개 통신 규격 | — | 아직 없음 |
| V-Archive ID / 로컬 기록 결합 | ✅ | 부분적 |

- 로컬 `Recommender`(`rust/overmax_data/src/service/recommend.rs`)는 **floor 근접 패턴** 하나만 제공한다.
- UI는 `NativeApp::recommend_for_state` → `Recommender::recommend(song_id, mode, diff, …)` 단일 경로로 고정되어 있다.
- 커뮤니티 추천 품질은 높지만, **스크린 수가 적은 유저**에게 alt-tab UX는 치명적이다.
- 행이봇·로페봇·디맥지지는 자체 추천 로직이 이미 있지만, 외부에서 호출할 수 있는 규격은 없다.
  → Overmax는 이 추천 결과를 오버레이로 가져오기 위해 **프로토콜을 제안**하고, 커뮤니티 측에서
  채택하기 쉽도록 최소 구현 비용으로 설계한다.

### 목표

1. **로컬 Floor Recommender 유지** — 기본값·오프라인·저지연 보장.
2. **추천 소스 주입** — 동일 UI에 로컬·외부 소스를 병합 표시.
3. **Vary 기반 협상** — 소스가 반응하는 컨텍스트 차원을 `vary`로 선언. `vary = []`이면 컨텍스트와 무관한 고정 추천.
4. **Overmax = Viewer** — 인식·기록·메타 enrichment는 호스트 책임, **순수 추천 함수는 소스에 위임**.
5. **Footer는 로컬 전용** — `avg_rate` / `n/m개 패턴` 통계는 로컬 floor recommender에서만 계산. 외부 프로토콜·`RecommendBundle`에 포함하지 않음.

### 비목표

- 행이봇/로페봇/디맥지지 직접 연동 구현
- 추천 알고리즘 자체의 대규모 개선
- 인게임(플레이 중) 실시간 추천 갱신 — 선곡 화면(`is_song_select`)에서만 호출

### 대안 검토

| 접근 | 설명 | 채택 여부 | 이유 |
|------|------|-----------|------|
| **HTTP 프로토콜 (선택)** | 외부 서비스가 HTTP 엔드포인트로 추천을 노출, Overmax가 호출 | ✅ | 언어 무관, 구현 부담 최소, 기존 인프라(캐시/스레드) 재사용 가능 |
| 직접 프로세스 연동 | 외부 바이너리를 자식 프로세스로 실행, stdin/stdout으로 통신 | ❌ | 보안 위험, 크로스 플랫폼 이슈, 프로세스 관리 복잡 |
| 공유 파일 폴링 | 외부 서비스가 파일에 추천 결과를 쓰고, Overmax가 폴링 | ❌ | 실시간성 부족, 파일 잠금/손상 위험, 디텍션 트리거와 분리 관리 어려움 |
| IPC (named pipe/shared memory) | OS 단위 IPC 메커니즘으로 통신 | ❌ | 언어·플랫폼 의존성 높음, 유지보수 부담 |
| 내장 추천 알고리즘 확장 | Overmax 자체에 다양한 추천 알고리즘을 추가 구현 | ❌ | 유지보수 부담, Overmax의 핵심 역량이 아님 |

**선택 근거**: Overmax의 핵심 가치는 **화면 인식·컨텍스트 파악·오버레이 렌더**이다.
추천 알고리즘 자체는 커뮤니티의 영역이며, Overmax는 **가장 얇은 다리**만 제공하는 것이
역할 분담에 맞다. HTTP GET + JSON은 그 "가장 얇은 다리"를 실현하는 가장 실용적인 방식이다.

---

## 2. 설계 원칙

1. **구현 비용 최소화**: provider 쪽 최소 구현은 `/recommend` 엔드포인트 하나로 충분해야 한다.
   manifest, vary 선언 등은 전부 optional 최적화이며 없어도 동작해야 한다.
2. **로컬 baseline 유지**: 네트워크 실패, provider 미설정, 응답 스펙 불일치 등 모든 실패 케이스는
   조용히 로컬 `Recommender`로 폴백한다. 사용자에게 에러를 노출하지 않는다.
3. **인게임 성능 영향 없음**: provider 호출은 절대 오버레이 렌더 루프를 블로킹하지 않는다.
   기존 `varchive_upload::fetch_records_blocking` + `spawn_fetch` 패턴(백그라운드 스레드 → 디스크
   캐시 파일 → repaint)을 그대로 재사용한다. 신규 인프라(비동기 런타임 등)를 도입하지 않는다.
4. **언어 무관**: provider 구현 언어는 Overmax의 Rust 스택과 무관하다. 순수 HTTP + JSON으로만 통신한다.
5. **버전 협상은 관대하게(lenient)**: 모르는 필드는 무시하고, protocol 버전이 안 맞으면 강제
   업데이트를 요구하지 않고 그냥 해당 provider를 비활성 취급한다.
6. **Footer 분리**: `RecommendResult`의 `avg_rate` / `has_record_count` / `total_count` 등
   오버레이 하단 통계는 로컬 floor recommender 전용이다. 외부 소스 결과에는 이 필드가 없다.
7. **설정은 GUI로**: `settings.user.json` 직접 편집을 일반 사용자에게 요구하지 않는다.
   기존 설정 다이얼로그(탭 형태)에 추천 provider 설정 섹션을 추가한다.

---

## 3. 핵심 타입 설계

### 3.1. `RecommendContext`

호스트가 소스에게 전달하는 컨텍스트 캡슐.

```rust
#[derive(Clone, Debug)]
pub struct RecommendContext {
    pub song_id: i32,
    pub button_mode: String,
    pub difficulty: String,
    pub v_id: Option<String>,
}
```

### 3.2. `VaryDim` — 협상 단위

소스가 반응하는 컨텍스트 차원을 나타낸다. `vary` 선언은 이 값들의 부분집합이다.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VaryDim { SongId, Mode, Diff, VId }
```

**협상 규칙**

- 소스 manifest의 `vary`에 포함된 차원만 캐시 키·재요청 트리거에 사용한다.
- `vary`에 없는 차원도 호출 시 **전송은 하지만** 캐시 판단에는 반영하지 않는다 (provider가 참고할 수 있게).
- `vary = []`이면 컨텍스트와 무관한 고정 추천이다. 곡/모드/난이도가 바뀌어도 네트워크 재요청이 전혀 발생하지 않는다.

### 3.3. `RecommendBundle` — 소스별 **목록만**

외부·로컬 구분 없이 소스는 **entries + status** 만 반환한다. Footer는 여기에 넣지 않는다.

```rust
#[derive(Debug, Clone)]
pub struct RecommendBundle {
    pub source_id: String,
    pub source_label: String,
    pub entries: Vec<RecommendEntry>,
    pub status: SourceStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceStatus {
    Ok,
    Stale,     // 캐시된 이전 결과 (TTL 만료 등)
    Skipped,   // 컨텍스트 부족으로 호출하지 않음
    Error,     // fetch/parse 실패
}
```

### 3.4. `LocalRecommendFooter` — 로컬 전용 통계

기존 `RecommendResult`의 `avg_rate` / `has_record_count` / `total_count`는 **floor 대역 요약**으로,
로컬 floor recommender의 `get_summary_from_cache`에서만 산출된다. UI 하단 표시 전용.

```rust
#[derive(Clone, Debug, Default)]
pub struct LocalRecommendFooter {
    pub avg_rate: f64,           // -1.0 = 기록 없음
    pub has_record_count: usize,
    pub total_count: usize,
}
```

### 3.5. `RecommendPanel` — UI가 소비하는 aggregate

```rust
pub struct RecommendPanel {
    pub bundles: Vec<RecommendBundle>,
    pub local_footer: Option<LocalRecommendFooter>,
}
```

**규칙**

| 데이터 | 출처 | 외부 프로토콜 | UI 위치 |
|--------|------|---------------|---------|
| `entries[]` | 모든 소스 | ✅ | 소스별 섹션 행 |
| `entry.rate` / MC | Host enrichment (RecordDB) | ❌ (Host가 merge) | 행 우측 |
| `local_footer` | 로컬 floor recommender only | ❌ **프로토콜 필드 없음** | 패널 글로벌 헤더 |

- 외부 소스 섹션 헤더에는 `avg` / `n/m` **표시하지 않는다** (의미 없는 숫자 방지).
- 로컬 floor가 disabled면 `local_footer = None` → 헤더 통계 영역 숨김 또는 `—`.

### 3.6. `RecommendResult` (기존, 유지)

```rust
pub struct RecommendResult {
    pub entries: Vec<RecommendEntry>,
    pub avg_rate: f64,
    pub has_record_count: usize,
    pub total_count: usize,
}
```

기존 UI 코드와의 호환성을 위해 유지한다. Phase 1에서 `RecommendPanel`으로 전환할 때
`as_legacy_result()` projection을 제공한다.

### 3.7. `RecommendationSource` trait

```rust
pub trait RecommendationSource: Send + Sync {
    fn source_label(&self) -> &str;
    fn recommend(&self, ctx: &RecommendContext) -> RecommendBundle;
}
```

- 반환 타입이 `Option<…`이 아니다. 소스는 항상 `RecommendBundle`을 반환하고,
  `status` 필드로 결과 상태를 표현한다.
- 로컬 `LocalFloorRecommender`는 `recommend()`로 entries만 반환하고,
  `floor_summary()` 별도 메서드로 `LocalRecommendFooter`를 산출한다.

---

## 4. 두 단계 엔드포인트

### 4.1. Manifest (선택, 드물게 fetch)

provider가 자신의 특성을 선언한다. 없거나 실패해도 기본값으로 동작 가능해야 한다.

```
GET {provider_url}/manifest
```

응답:

```json
{
  "protocol": "overmax-recommend/1",
  "name": "djmax.gg",
  "vary": ["mode"],
  "ttl_sec": 3600,
  "endpoint": "/recommend"
}
```

필드:

| 필드 | 타입 | 설명 |
|---|---|---|
| `protocol` | string | 고정 문자열 `"overmax-recommend/1"`. 다르면 해당 provider를 무시한다. |
| `name` | string | 표시용 이름 (예: 오버레이 UI에 아주 작게 노출될 수 있음, 현재는 미사용) |
| `vary` | string[] | 추천 결과가 반응하는 컨텍스트 차원. `"song_id"`, `"mode"`, `"diff"`, `"v_id"`의 부분집합. 빈 배열이면 컨텍스트와 완전 무관한 고정 추천(예: "오늘의 이치오시"). |
| `ttl_sec` | number | 캐시 유효 시간(초). 없으면 기본값 3600 적용. |
| `endpoint` | string | 상대 또는 절대 경로. 없으면 `{provider_url}/recommend`를 기본값으로 가정. |

**manifest fetch 실패 시 기본값**: `vary = ["song_id", "mode", "diff"]`, `ttl_sec = 3600`,
`endpoint = {provider_url}/recommend` (가장 보수적인 기존 동작과 동일하게 취급).

### 4.2. Recommend (필수)

```
GET {endpoint}?song_id={id}&mode={4B|5B|6B|8B}&diff={NM|HD|MX|SC}&v_id={v_id}
```

- 쿼리 파라미터는 `vary`에 없는 차원도 항상 전부 전송한다 (provider가 원하면 참고할 수 있게).
  단, 캐시/재요청 판단은 Overmax 쪽에서 `vary` 기준으로만 한다.
- `v_id`는 사용자가 V-Archive 계정을 연동하지 않았으면 빈 문자열로 보낸다.

응답:

```json
{
  "protocol": "overmax-recommend/1",
  "source": "djmax.gg",
  "entries": [
    {
      "song_id": 123,
      "mode": "5B",
      "diff": "SC",
      "reason": "similar_tag",
      "score": 0.87
    }
  ]
}
```

필드:

| 필드 | 타입 | 설명 |
|---|---|---|
| `protocol` | string | `"overmax-recommend/1"` 고정. 다르면 결과 전체를 무시. |
| `source` | string | provider 식별용 문자열. |
| `entries` | array | 추천 곡 목록. 최대 개수 제한은 Overmax 쪽에서 표시 시점에 자름(현재 6개). |
| `entries[].song_id` | number | V-Archive 곡 ID (기존 `songs.json`의 `title` 필드와 동일 체계). |
| `entries[].mode` | string | `"4B"`/`"5B"`/`"6B"`/`"8B"`. |
| `entries[].diff` | string | `"NM"`/`"HD"`/`"MX"`/`"SC"`. |
| `entries[].reason` | string? | 선택. 추천 사유 라벨(자유 문자열). 현재 UI에서 미사용, 향후 확장 여지로 예약. |
| `entries[].score` | number? | 선택. 0.0~1.0 권장이나 강제하지 않음. 현재 UI에서 미사용. |

> **Protocol Boundary**: 위 `entries`가 프로토콜의 전체 스펙이다. `RecommendResult`의
> `avg_rate`, `has_record_count`, `total_count` 등 오버레이 footer 통계는 로컬
> `LocalFloorRecommender`에서만 계산되며 프로토콜에 포함되지 않는다.

`song_id`가 로컬 `songs.json`에 없는 값이면 해당 entry는 조용히 drop한다 (전체 응답을 무효화하지 않음).

---

## 5. Vary 기반 캐시 키 및 재요청 트리거

`vary` 선언에 포함된 차원만으로 캐시 키를 구성한다. 이 캐시 키가 곧 재요청 트리거 조건이다.

```
vary = []                          -> cache_key = "global"
vary = ["mode"]                    -> cache_key = "{mode}"
vary = ["song_id","mode","diff"]   -> cache_key = "{song_id}_{mode}_{diff}"
vary = ["v_id"]                    -> cache_key = "{v_id}"
```

- `vary = []`인 provider("오늘의 이치오시" 류)는 곡/모드/난이도가 바뀌어도 네트워크 재요청이
  전혀 발생하지 않는다. TTL 만료 시에만 재요청.
- 캐시 키가 바뀌지 않는 한 재요청하지 않는다 (기존 `Changed<T>` 패턴으로 감시).

---

## 6. 캐시 저장 위치 및 TTL

기존 `cache/varchive/{steam_id}/{button}.json` 패턴과 동일하게, 파일 기반 캐시를 사용한다.

```
cache/recommend_provider/{provider_name}/{cache_key}.json
```

- 파일의 mtime을 TTL 판단 기준으로 사용한다 (`cache_update.rs::is_stale`와 동일 로직 재사용).
- TTL 만료 시 조회 자체는 즉시 실패 처리(로컬로 폴백)하고, 백그라운드로 갱신 요청을 별도로
  트리거한다. 즉 "느린 네트워크 응답을 기다리며 프레임을 멈추는" 상황을 만들지 않는다.

---

## 7. 실패 처리 규칙

다음 중 하나라도 해당하면 해당 provider 조회는 `SourceStatus::Error`를 반환하고,
상위 `CompositeRecommender`가 로컬 `LocalFloorRecommender`로 폴백한다:

- 캐시 파일이 없음 (아직 최초 fetch 전, 또는 provider 비활성)
- 캐시 파일이 TTL 만료
- 캐시 파일 파싱 실패 (JSON 스키마 불일치 포함)
- `protocol` 필드 불일치
- `entries`가 비어 있음

에러를 사용자에게 노출하지 않는다. 디버그 로그(`[Recommend]` 태그)로만 남긴다.

---

## 8. Overmax 측 구현 가이드

### 8.1. 계층 배치

`overmax_data`는 네트워크 I/O를 하지 않는다 (지금도 `reqwest` 의존성이 없음). 네트워크는
`overmax_app`이 백그라운드 스레드로 수행하고 디스크 캐시 파일만 남긴다. `overmax_data`는 그
캐시 파일을 읽기만 한다.

```
overmax_app    : provider fetch (신규 system/recommend_provider_fetch.rs, varchive_upload.rs와 대칭)
overmax_data   : RecommendationSource trait, RecommendBundle, LocalRecommendFooter, CompositeRecommender
```

### 8.2. 타입 스케치 (`overmax_data/src/service/recommend.rs` 또는 신규 `recommend_provider.rs`)

```rust
#[derive(Clone, Debug)]
pub struct RecommendContext {
    pub song_id: i32,
    pub button_mode: String,
    pub difficulty: String,
    pub v_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VaryDim { SongId, Mode, Diff, VId }

#[derive(Debug, Clone)]
pub struct RecommendBundle {
    pub source_id: String,
    pub source_label: String,
    pub entries: Vec<RecommendEntry>,
    pub status: SourceStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceStatus { Ok, Stale, Skipped, Error }

#[derive(Clone, Debug, Default)]
pub struct LocalRecommendFooter {
    pub avg_rate: f64,
    pub has_record_count: usize,
    pub total_count: usize,
}

pub struct RecommendPanel {
    pub bundles: Vec<RecommendBundle>,
    pub local_footer: Option<LocalRecommendFooter>,
}

pub trait RecommendationSource: Send + Sync {
    fn source_label(&self) -> &str;
    fn recommend(&self, ctx: &RecommendContext) -> RecommendBundle;
}

// 기존 Recommender에 trait만 씌움 (구조 변경 없음)
impl RecommendationSource for LocalFloorRecommender { ... }

pub struct ProviderCacheReader {
    cache_dir: PathBuf,
    vary: Vec<VaryDim>,
    ttl: Duration,
}
impl RecommendationSource for ProviderCacheReader { ... }

pub struct CompositeRecommender {
    provider: Option<ProviderCacheReader>,
    local: LocalFloorRecommender,
}
impl CompositeRecommender {
    pub fn recommend(&self, ctx: &RecommendContext) -> RecommendPanel {
        let local_bundle = self.local.recommend(ctx);
        let local_footer = self.local.floor_summary(ctx);
        let provider_bundle = self.provider
            .as_ref()
            .map(|p| p.recommend(ctx));

        let bundles = match provider_bundle {
            Some(b) if b.status == SourceStatus::Ok && !b.entries.is_empty() => vec![b, local_bundle],
            _ => vec![local_bundle],
        };

        RecommendPanel { bundles, local_footer: Some(local_footer) }
    }
}
```

### 8.3. 설정 스키마 (`overmax_data/src/config/settings.rs`)

```rust
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RecommendProviderSettings {
    #[serde(default)]
    pub enabled: bool,
    pub url: Option<String>,
    pub name: Option<String>,
}
```

`settings.json` 기본값:

```json
"recommend_provider": {
  "enabled": false,
  "url": null,
  "name": null
}
```

manifest에서 받은 `ttl_sec`/`vary`/`endpoint`는 설정 파일이 아니라 런타임에 fetch해서 메모리에
캐시한다 (사용자가 직접 편집하는 값이 아니므로 `settings.json`에 넣지 않는다).

### 8.4. 설정 UI (`rust/overmax_app/src/ui/settings_ui.rs`)

일반 사용자가 `settings.user.json`을 직접 편집하게 하지 않는다. 기존 설정 다이얼로그의
System 탭에 **추천 Provider** 섹션을 추가한다.

```
설정 다이얼로그
├── UI 탭 (기존)
├── V-Archive 탭 (기존)
└── System 탭
    ├── 업데이트 설정 (기존)
    └── 추천 Provider (신규)
        ├── 사용 [토글]
        ├── Provider URL [텍스트 입력]
        ├── 표시 이름 [텍스트 입력]
        ├── [연결 테스트] 버튼
        └── 상태: 마지막 성공 {시간} / 오류: {메시지}
```

**구현 방식**:
- 기존 `system_tab()` 함수 내에 `recommend_provider_section()` 추가
- `form_row()` 헬퍼로 레이블-컨트롤 정렬 유지
- `checkbox` / `TextEdit` / `Button` egui 위젯 사용 (현재 설정 UI와 동일 패턴)
- "연결 테스트" 버튼은 비동기 Worker 채널로 테스트 요청을 보내고, 결과를 상태 텍스트로 표시
- 변경은 `save_settings_to_disk()`로 즉시 `settings.user.json`에 반영 (delta 형식)

### 8.5. Fetch 트리거 (`overmax_app`)

`native_app_recommend.rs::refresh_overlay_data()`가 지금 song/mode/diff 변경 시 로컬 추천을
갱신하는데, 여기에 provider fetch 트리거를 얹는다. 감시 대상은 `RecommendContext` 전체가
아니라 **provider의 `vary`로 유도한 cache_key**여야 한다 (`Changed<String>`로 감시).

기존 `SyncWorkerChannels`의 `fetch_req_tx`/`fetch_res_rx` 채널 페어와 동일한 모양으로
provider 전용 채널을 하나 추가하거나, 범용화해서 재사용한다. 스레드 본체는
`varchive_upload::fetch_records_blocking` + `NativeApp::spawn_fetch`를 거의 그대로 복제-치환.

### 8.6. 최초 manifest fetch 타이밍

앱 시작 시 `provider.enabled == true`이면 1회 manifest fetch를 시도한다
(`cache_update::refresh_startup_caches`와 같은 스타트업 갱신 목록에 추가). 실패해도 앱 기동을
막지 않으며, 위에 정의한 기본값으로 동작한다.

---

## 9. UI 방향 (정적 표시, 인터랙션 없음)

오버레이 패널은 외부 자극(컨텍스트 변경, provider 데이터 갱신)에 반응해 내용을 갱신하는 **수동 표시** 역할만 한다.
사용자 인터랙션(소스 전환, 접기/펼침, 필터, 정렬 등)은 설계하지 않는다.

### 9.1. 데이터 흐름

```
CompositeRecommender
    → RecommendPanel { bundles, local_footer }
    → as_legacy_result() → RecommendResult { entries, avg_rate, has_record_count, total_count }
    → draw_recommendations() / draw_footer()  // 기존 UI 그대로
```

- `entries`는 로컬·provider 구분 없이 **단일 flat list**로 병합되어 들어온다
- `local_footer`는 항상 로컬 floor recommender의 통계를 사용한다
- UI는 `entries`와 `footer`만 보고 렌더하므로, source 출처를 알 필요가 없다

### 9.2. 엔트리 소스 구분 (시각적 단서)

사용자가 혼란을 느끼지 않도록, provider 유래 엔트리에 **미묘한 시각적 단서**를 줄 수 있다.
인터랙션은 없고, 정보 전달만 목적이다.

| 방식 | 구현 | 비고 |
|------|------|------|
| **소스 라벨 배지** | `entry.source_label`을 song name 우측이나 rate 영역에 작은 텍스트로 표시 | 가장 명확하지만 공간 차지 |
| **텍스트 색상 변경** | provider entry의 song_name을 `Theme::TEXT_SECONDARY`로 | 미묘, 공간 안 씀 |
| **점 표시** | rate 옆에 2px 컬러 닷 | 최소 공간, "정보가 있다" 정도의 암시 |

**권장**: 아무 표시도 하지 않거나, 필요하면 텍스트 색상 변경 정도. 인터랙션이 없으므로
"어디서 온 건지 알 필요 없다"는 접근이 가장 일관성 있다.

### 9.3. Stale 상태 표시

provider 데이터가 TTL 만료 등으로 `Stale` 상태일 때, 기존 캐시 entries를 계속 표시한다.
이 경우 **시각적 구분 없이** 그대로 표시한다. (stale indicator는 디버그 로그만으로 충분)

### 9.4. 변경하지 않는 것

- `draw_recommendations()` 내부 루프 구조 (6행 truncate 포함)
- `draw_recommend_row()` 내부 배치 (badge | name | rate)
- `draw_footer()`의 avg_rate / pattern_count 표시 위치·형식
- `PatternTabInfo` 및 diff 탭 영역
- `Lite Mode` 동작

### 9.5. 초안 요약

```
┌─────────────────────────────────────────┐
│ [SC 12.3] Song A               99.12% M │  ← 로컬 또는 provider entry, 시각적 구분 없음
│ [5B Lv8]  Song B               98.50%   │
│ [MX]      Song C               97.80%   │
│ ...                                     │
├─────────────────────────────────────────┤
│ 유사 구간 평균   3/12개 패턴 · 4.2%    │  ← 항상 로컬 footer
└─────────────────────────────────────────┘
```

provider 유무를 사용자가 직접 구분할 수단은 없다. 단, settings에서 provider를 끄면
자연스럽게 로컬 entries만 표시되는 식으로 **간접적**으로만 반영된다.

### 9.6. 호환성 projection

```rust
impl RecommendPanel {
    /// Phase 1~2: overlay_ui 헤더 + 단일 소스 entries 호환
    pub fn as_legacy_result(&self) -> RecommendResult {
        let entries = self.bundles.first()
            .map(|b| b.entries.clone())
            .unwrap_or_default();
        let footer = &self.local_footer;
        RecommendResult {
            entries,
            avg_rate: footer.as_ref().map(|f| f.avg_rate).unwrap_or(-1.0),
            has_record_count: footer.as_ref().map(|f| f.has_record_count).unwrap_or(0),
            total_count: footer.as_ref().map(|f| f.total_count).unwrap_or(0),
        }
    }
}
```

---

## 10. 보안 & 프라이버시

| 위험 | 대책 |
|------|------|
| SSRF (악성 endpoint) | v1에서는 localhost만 허용 (`127.0.0.1`, `::1`). settings에서 URL scheme/host 검증. (Overmax는 이미 Google Sheets·V-Archive API 등 원격 호출을 하고 있으므로, 이는 v1 보수적 시작점일 뿐 영구적 제한은 아님) |
| 과도한 개인 데이터 유출 | 기본적으로 `v_id` 외 개인정보 미전송. provider가 추가 필드 요구시 수동 확인. |
| 악성 JSON (거대 payload) | `max_entries` 상한 (Host 32, UI 6), body size cap 64KB. |
| 자식 프로세스 실행 | v1 범위 밖. HTTP만 지원. |

---

## 11. 트레이드오프

| 선택 | 장점 | 단점 |
|------|------|------|
| HTTP GET + Vary | 단순, 언어 무관, mock 쉬움 | POST body 복잡한 협상 불가 (v1에서는 불필요) |
| Vary 기반 캐시 | provider 부담 최소, 관대한 협상 | `vary` 외 차원은 캐시 무시 |
| Footer 로컬 분리 | floor 대역 통계 의미 보존, 프로토콜 단순화 | local.floor off 시 헤더 통계 공백 |
| 단일 provider | 오버레이 높이 일관성 | 다중 소스 다양성 부족 |
| 파일 캐시 | 기존 인프라 재사용, TTL 기반 자연 갱신 | 디스크 I/O 추가 (경미함) |

---

## 12. 명시적으로 미결정/범위 밖 (Out of Scope)

- **UI는 정적 표시만 (인터랙션 없음)**: 오버레이 패널은 외부 자극에 반응해 내용을 갱신하는 **수동 표시** 역할만 한다.
  소스 전환, 접기/펼침, 필터, 정렬 등 어떤 사용자 인터랙션도 설계하지 않는다.
  다중 소스가 연결되더라도 표시 형태는 현재 6줄 리스트를 유지하며, 새로운 UI 요소는 추가하지 않는다.
  (레이아웃·폰트·색상 등 시각적 미세 조정은 가능하나, 구조적 변경은 없다.)
- **`reason`/`score` 필드의 UI 활용**: 스키마에는 예약해두되 현재는 표시하지 않는다.
- **인증/API 키**: v1에는 없음. provider가 필요로 하면 추후 헤더 기반으로 확장.
- **요청 빈도 제한(rate limit) 정책**: provider 쪽 부담을 주지 않기 위한 최소 fetch 빈도는
  TTL로만 제어한다. 별도 exponential backoff 등은 v1 범위 밖.

---

## 13. 구현 단계 (Incremental Migration)

ENGINEERING_TASTE.md 원칙: **동작 중인 floor recommender를 깨지 않는 선에서** 단계적 도입.

### Phase 1 — In-process 추상화 (로컬만)

**목표**: 외부 API 없이 trait + registry 도입. 사용자 체감 변화 없음. **UI 파일은 전혀 건드리지 않는다**.

1. `recommend.rs` → `recommend/local_floor.rs` 이동, `LocalFloorRecommender impl RecommendationSource`
2. `RecommendContext` / `VaryDim` / `RecommendBundle` / `SourceStatus` / `LocalRecommendFooter` / `RecommendPanel` 추가
3. `LocalFloorRecommender`: `recommend()`와 `floor_summary()` 분리 (기존 `RecommendResult` 후자 필드 → footer)
4. `ProviderCacheReader` + `CompositeRecommender` 신규 추가, 유닛 테스트
   (캐시 없음/만료/파싱실패/protocol 불일치 각각 로컬 폴백 확인)
5. `native_app_recommend.rs`: 데이터 계층만 `RecommendPanel`으로 전환, UI는 `as_legacy_result()` projection으로 기존 `RecommendResult` 흘려보냄 (UI 파일 변경 없음)
6. `cargo test --workspace` 통과

**검증**: floor 추천 결과가 refactor 전후 동일.

### Phase 2 — Fetch 인프라

1. `system/recommend_provider_fetch.rs` 신규 (manifest + recommend 블로킹 fetch 함수)
2. `NativeApp`에 provider fetch 채널 추가, `refresh_overlay_data()`에 트리거 훅
3. 스타트업 시 manifest 1회 fetch 훅 (`cache_update.rs` 스타트업 목록에 추가)
4. `RecommendProviderSettings` 설정 스키마 추가, `settings.json` 기본값 반영
5. `settings_ui.rs` System 탭에 추천 Provider 섹션 추가 (토글, URL 입력, 연결 테스트, 상태 표시)
6. 수동 검증: provider 미설정 상태에서 기존 동작과 100% 동일한지 회귀 확인

### Phase 3 — 다중 소스 UI

1. `overlay_recommend_ui.rs` 다중 bundle 렌더 (소스별 섹션)
2. 접기/순서/활성화 settings UX
3. `Lite Mode` 정책 확정

### Phase 4 — 커뮤니티 문서 공개

1. `docs/overmax-recommend-protocol-v1.md` — 외부용 slim spec (한/영)
2. 예제 Python mock server (`examples/recommend_mock_server.py`)
3. README에 "Community Recommender" 섹션

---

## 14. 체크리스트 (구현 순서 제안)

1. `overmax_data`: `RecommendContext`, `VaryDim`, `RecommendationSource` trait 추가
2. `overmax_data`: 기존 `Recommender` → `LocalFloorRecommender` rename + trait impl (구조 변경 없이 wrapper만)
3. `overmax_data`: `RecommendBundle`, `SourceStatus`, `LocalRecommendFooter`, `RecommendPanel` 추가
4. `overmax_data`: `ProviderCacheReader` + `CompositeRecommender` 신규 추가, 유닛 테스트
5. `overmax_app`: `system/recommend_provider_fetch.rs` 신규 (manifest + recommend 블로킹 fetch 함수)
6. `overmax_app`: `NativeApp`에 provider fetch 채널 추가, `refresh_overlay_data()`에 트리거 훅
7. `overmax_app`: 스타트업 시 manifest 1회 fetch 훅 (`cache_update.rs` 스타트업 목록에 추가)
8. 수동 검증: provider 미설정 상태에서 기존 동작과 100% 동일한지 회귀 확인
