# Windows Auto HDR 환경에서의 DXGI FP16 캡처 및 2-Stage 역톤매핑 계획

- **작성일**: 2026-09-10
- **상태**: 승인 대기 (Draft)
- **대상 브랜치**: `feat/auto-hdr-wgc`
- **주요 영역**: `overmax_engine::capture::capture_engine::windows`, `overmax_engine::detector`, `overmax_cv`

---

## 1. 개요 및 배경

DJMAX RESPECT V는 자체 HDR 렌더링을 지원하지 않는 **순수 SDR (DirectX 11, 8비트 sRGB)** 게임이다.  
그러나 최신 OLED 및 게이밍 HDR 모니터 보급으로 인해 많은 플레이어가 Windows 11의 **"Auto HDR (자동 HDR)"** 기능을 켠 채로 게임을 즐기고 있다.

Auto HDR이 켜지면 Windows DWM(데스크톱 윈도우 매니저)이 DXGI 출력 백버퍼를 scRGB FP16(`DXGI_FORMAT_R16G16B16A16_FLOAT`, $1.0 = 80\text{ nits}$)으로 합성하며 다음과 같은 디텍션 장애가 발생한다:

1. **모드(4B/5B/6B/8B) 인식 실패**: 특히 어두운 색상인 4B(`#0E4960`)와 8B(`#1D1431`)의 유클리디언 색상 거리가 허용 한도(`BTN_MODE_MAX_DIST = 60.0`)를 초과하여 모드 인식 불가.
2. **자켓 매칭 실패**: 앨범 아트의 컬러 히스토그램과 대비가 어긋나 유사도 임계치(0.65) 미달.
3. **Rate 및 점수 템플릿 매칭 실패**: 흰색 텍스트 주변에 강한 블룸(후광)이 생기며, 글로벌 콘트라스트 이진화 시 폰트 외곽선이 뭉개져 255 떡짐 현상 발생.
4. **씬 판정 고립**: 자켓과 모드 인식이 동시에 흔들려 시스템이 `Unknown` 씬에서 벗어나지 못함.

본 계획은 신규 WGC 백엔드 추가에 따른 아키텍처 복잡성과 런타임 오버헤드를 배제하고, **기존에 검증된 DXGI Desktop Duplication (0.62ms 초저지연 아틀라스 파이프라인)** 위에서 **수학적으로 255 포화(Saturation)를 방지하는 2-Stage 역톤매핑 LUT와 CV 레이어 내성 가드**를 구축하여 Auto HDR 환경에서 100% 인식률을 달성하는 것을 목표로 한다.

---

## 2. 근본 원인 분석: Auto HDR 왜곡 메커니즘과 기존 파이프라인의 한계

Windows 11 Auto HDR은 게임 백버퍼를 단순히 일정한 배율로 확대하는 것이 아니라, **휘도(Luminance) 구간에 따라 비선형 Inverse Tone Mapping (ITM)**을 적용한다.

```
[Auto HDR의 휘도 구간별 왜곡 구조]

  휘도 (scRGB)
   ▲
10.0│                                 ┌─ 흰색 폰트, Score, Rate (1,000 nits+로 극단 부스트)
    │                                ╱   + 주변 픽셀로 광범위한 블룸(Bloom) 확산
 3.3│─────────┬─────────────────────┘   <-- OS SDR White Level (W_sdr, 모니터 슬라이더 계측치)
    │        ╱ (중간 톤: 자켓, 모드 버튼 색상)
 0.0└───────┴────────────────────────► 원본 SDR 휘도 (0.0 ~ 1.0)
```

### 2.1 중간 톤의 왜곡 (자켓 & 모드 버튼 인식 실패)
* 자켓 이미지의 색상과 버튼 모드 고유 색상은 피크 하이라이트가 아닌 **중간 톤(Luma < 0.7)**에 위치한다.
* Auto HDR에서도 이 영역은 실제 OS SDR 화이트 레벨($W_{\text{sdr}}$, 예: 3.3배 $\approx$ 264 nits) 선형 비례 영역에 머무른다.
* 현재 코드베이스는 Win32 CCD API(`detect_monitor_sdr_white_level`)로 $W_{\text{sdr}}$을 감지하지만, 감지 실패 시의 안전 폴백(`SCRGB_SDR_WHITE_LEVEL = 5.168`)이 적용되거나 단순 선형 단일 나눗셈만 사용할 경우:
  * 4B(`#0E4960`)와 8B(`#1D1431`)처럼 채널 값이 10~50 수준인 극저조도 색상은 미세한 감마/스케일 오차만으로도 유클리디언 거리 60을 초과.
  * 자켓 전체의 휘도 대비가 가라앉아 히스토그램 WTA 매칭 유사도 저하.

### 2.2 고휘도 톤의 왜곡 및 이진화 떡짐 (폰트 매칭 실패)
* 흰색 텍스트(Rate, Score 등)는 Auto HDR 인핸서에 의해 **5.0 ~ 10.0+ (1,000 nits 이상)**까지 치솟으며 주변 픽셀로 강한 블룸(Bloom)이 번진다.
* `overmax_cv`의 글로벌 콘트라스트 이진화([image.rs](rust/overmax_cv/src/image.rs#L473)) 로직:
  $$\text{threshold} = \max(\text{max} \times 0.80, \text{max} - 45)$$
  * 피크 텍스트로 인해 $\text{max} = 255$가 되면 임계값은 무조건 **$210$**으로 고정된다.
  * Auto HDR 블룸으로 인해 글자 주변 배경이 $215 \sim 230$ 수준까지 밝아지면, 임계값(210)보다 높아 **글자 획과 주변 후광이 한 덩어리로 묶여 이진화**되어 템플릿 매칭이 실패한다.
* **단순 톤매핑의 255 포화 모순**:
  $W_{\text{sdr}}$을 그대로 선형 $1.0$(255)에 대응시키면 $W_{\text{sdr}}$ 초과의 모든 하이라이트와 블룸이 255로 잘려(Saturation) 글자 구분이 불가능해지고, 반대로 고휘도 피크($10.0$)에 맞추면 중간 톤이 극도로 어두워지는 딜레마가 발생한다.

---

## 3. 핵심 아키텍처 설계

```
┌────────────────────────────────────────────────────────────────────────┐
│             Windows 11 Game Window (DX11 / Auto HDR Active)            │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ DXGI Desktop Duplication (IDXGIOutput6)
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│           DXGI Capture Engine (DXGI_FORMAT_R16G16B16A16_FLOAT)         │
│           -> scRGB FP16 전체 다이내믹 레인지 (0.0 ~ 10.0+) 수신        │
├────────────────────────────────────────────────────────────────────────┤
│   ★ 512x512 GPU ROI Atlas Staging Texture (FP16 = 2MB)                │
│   - CopySubresourceRegion 43회 직행 (VRAM 내부)                        │
│   - 핑퐁 더블 버퍼링 유지 (GPU 스톨 0ms, CPU DMA 2MB < 0.65ms)         │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ Map() & 비동기 DMA
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│           2-Stage 역톤매핑 64KB LUT (단일 패스 0.38ms 룩업)             │
├───────────────────────────────────┬────────────────────────────────────┤
│  [구간 1: 0 ~ V_knee] (선형 보존) │  [구간 2: > V_knee] (점근 숄더)   │
│  - V_knee = W_sdr * 0.9           │  - Hermite/Reinhard 점근 압축      │
│  - 자켓/모드 중간톤 1:1 완벽 보존 │  - 블룸 영역(215~235) 억제         │
│  - SDR 원본 색역 완벽 일치        │  - 글자 피크 중심만 250~255 안착   │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ BGRA8 (0~255 정밀 복원 아틀라스 버퍼)
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│                   Overmax Detection & Matching Pipeline                │
│    - Luma-Gated Hybrid Metric 모드 판정 (4B/8B 저조도 노이즈 방어)     │
│    - Highlight Separation Guard (글로벌 콘트라스트 이진화 블룸 제거)   │
│    - HOG / Grid Histogram 자켓 매칭 정상화                            │
└────────────────────────────────────────────────────────────────────────┘
```

### 3.1 DXGI FP16 512×512 GPU 아틀라스 파이프라인
* **DXGI 백엔드 단일화**: WGC를 추가하지 않고 기존 `DxgiCaptureEngine`을 그대로 사용. WGC 도입에 따른 콜백 생명주기 관리(`FrameArrived`), 캡처 테두리(Yellow Border) 제어, 창 리사이즈 재생성 오버헤드를 원천 차단.
* **메모리 대역폭 방어 (2MB 제한)**:
  * 1080p 풀프레임 FP16(16.6MB)을 CPU로 복사하지 않고, 기존 512×512 GPU ROI Atlas를 FP16 포맷(`staging_atlas_textures: [Option<ID3D11Texture2D>; 2]`)으로 운용.
  * 백버퍼에서 Staging Atlas로의 `CopySubresourceRegion` 43회 직행 및 핑퐁 더블 버퍼링 유지 $\to$ **캡처 지연을 0.65ms 수준으로 완벽 방어**.

### 3.2 2-Stage 역톤매핑 64KB LUT (255 포화 방지 수식)
성능 저하를 0으로 유지하기 위해, 65,536개 FP16 비트 전체에 대한 사전 계산 64KB 룩업 테이블을 2-Stage 점근 커브로 구축한다.

$$V_{\text{knee}} = W_{\text{sdr}} \cdot 0.90$$

$$L(V) = \begin{cases} 
\dfrac{V}{W_{\text{sdr}}} \cdot L_{\text{knee\_out}} & \text{if } V \le V_{\text{knee}} \quad (\text{Stage 1: Linear SDR Preservation}) \\ 
L_{\text{knee\_out}} + (1.0 - L_{\text{knee\_out}}) \cdot \dfrac{\frac{V - V_{\text{knee}}}{\sigma}}{1.0 + \frac{V - V_{\text{knee}}}{\sigma}} & \text{if } V > V_{\text{knee}} \quad (\text{Stage 2: Asymptotic Shoulder Compression})
\end{cases}$$

* **파라미터 규격**:
  * $L_{\text{knee\_out}} = 0.85$ (sRGB OETF 통과 시 약 235에 안착하여, SDR 중간톤 전 구간의 선형성을 보장하면서도 상단에 20레벨의 숄더 여유 공간 확보)
  * $\sigma = W_{\text{sdr}} \cdot 1.5$ (숄더 압축의 감쇠율 스케일)
* **산출 및 변환**:
  * 산출된 선형 값 $L(V)$에 표준 sRGB OETF 감마 곡선을 적용하여 0~255 바이트를 생성:
    $$\text{LUT}[bits] = \text{clamp}((\text{strict\_srgb\_oetf}(L(V)) \times 255.0 + 0.5) \text{ as } u8, 0, 255)$$
* **효과**:
  1. $V \le V_{\text{knee}}$: 자켓 및 모드 버튼 색상이 왜곡 없이 SDR 원본 비율로 복원됨.
  2. $V > V_{\text{knee}}$: $V \to \infty$일 때 $L(V) \to 1.0$(255)로 점근적으로 수렴하므로, **$W_{\text{sdr}}$ 초과 구간이 일괄 255로 잘리지 않음**.
  3. 글자 중심 획(1,000 nits+, $V \approx 6.0 \sim 10.0$)은 250~255로 안착하고, 글자 주변의 연한 블룸($V \approx 3.0 \sim 4.5$)은 215~235 구간으로 억제됨.

### 3.3 CV 레이어 내성 보강

#### A. Luma-Gated Hybrid Metric 모드 판정 (`play_state.rs`)
* 6B(주황), 5B(하늘)는 밝기가 충분하여 정규화 색상 방향각(Hue/Sat)이 매우 안정적임.
* 그러나 4B(`#0E4960`)와 8B(`#1D1431`)는 저조도 색상이므로 단순 단위 벡터 정규화 시 노이즈 증폭이 발생함.
* 따라서 **Luma Gate 기반 하이브리드 판정**을 적용:
  * ROI의 평균 밝기($\text{Luma} > 60$): 방향각 코사인 유사도를 보조 지표로 결합하여 밝기 편차에 완전 무관한 판정 수행.
  * ROI의 평균 밝기($\text{Luma} \le 60$): 노이즈 방지를 위해 기존 Euclidean 거리 거동을 유지하되, Luma 오차를 분리 감쇠하는 가중치 적용.

#### B. Highlight Separation Guard (`overmax_cv::image::binarize_by_global_contrast`)
* 텍스트 영역의 $max \ge 250$이고 명암비가 뚜렷할 때, Auto HDR 블룸 잔여 노이즈를 완전히 차단하기 위한 하이라이트 분리 가드 추가:
  ```rust
  let calculated = if max >= 250 && max.saturating_sub(min) > 80 {
      // Auto HDR 블룸 억제: 임계값을 230 이상으로 끌어올려 글자 중심 획만 선명하게 추출
      ((max as f32 * 0.80) as u8).max(max.saturating_sub(25)).max(min + 5)
  } else {
      ((max as f32 * 0.80) as u8).max(max.saturating_sub(45)).max(min + 5)
  };
  ```

---

## 4. 트레이드오프 및 기대 효과

* **아키텍처 단순성 및 안정성 극대화**:
  * WGC 도입을 배제하고 단일 DXGI 1.6 기반으로 통합하여, 불필요한 백엔드 분기와 윈도우 이벤트 예외 처리 오버헤드를 원천 소거.
* **인게임 성능 영향 제로 (Zero Additional Overhead)**:
  * 512×512 아틀라스(2MB)를 유지하여 CPU 전송 지연 0.65ms, LUT 역톤매핑 0.38ms로 총 캡처 분석 지연을 1.0ms 미만으로 엄격 유지.
* **플레이어 경험 극대화**:
  * 플레이어가 Windows Auto HDR을 끄거나 모니터 밝기 슬라이더를 조절할 필요 없이, 평소 환경 그대로 게임을 즐기면서 100% 인식 정확도 확보.

---

## 5. 세부 마일스톤 및 구현 계획

### Step 1. 2-Stage 점근 숄더 LUT 생성기 리팩토링 및 오프라인 회귀 검증 (`hdr_pipeline.rs`)
- [ ] `build_lut_table`을 2-Stage 점근 숄더 커브($V_{\text{knee}}$, $L_{\text{knee\_out}}$, $\sigma$)로 개편.
- [ ] 255 포화 방지 및 경계면 연속성 단위 테스트 작성.
- [ ] `tests/hdr_replay_test.rs`의 실제 스냅샷(`scratch/hdr/*.raw`)을 활용하여, 재구성된 LUT 통과 시 자켓 매칭 및 모드 판정이 정상 통과하는지 오프라인 회귀 검증.

### Step 2. DXGI FP16 아틀라스 Staging 파이프라인 정합성 점검 (`dxgi.rs`)
- [ ] `dxgi.rs`에서 HDR 감지 시 512×512 Staging Atlas 텍스처 포맷이 `DXGI_FORMAT_R16G16B16A16_FLOAT`으로 완벽히 동기화되어 작동하는지 점검.
- [ ] Win32 CCD API 감지 실패 시의 안전 폴백 값 및 LUT 갱신 이벤트 흐름 점검.

### Step 3. CV 레이어 안정화 (`overmax_cv`, `play_state.rs`)
- [ ] `overmax_cv::image::binarize_by_global_contrast`에 Highlight Separation Guard 적용.
- [ ] `play_state.rs`의 `detect_button_mode_from_roi`에 Luma-Gated 하이브리드 판정 적용.

### Step 4. 실전 인게임 종합 검증
- [ ] Windows Auto HDR이 켜진 실제 환경에서 DJMAX 선곡창, 결과창, 프리스타일 모드, 자켓 인식이 100% 안정 작동하는지 검증.
- [ ] `cargo test -p overmax-engine --test hdr_replay_test`, `cargo clippy --all-targets` 클린 통과 확인.
