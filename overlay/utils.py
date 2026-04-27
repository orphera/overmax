def calculate_overlay_position(target_x: int, target_y: int, window_width: int, window_height: int, screen_x: int, screen_y: int, screen_width: int, screen_height: int) -> tuple[int, int]:
    """
    오버레이 창이 화면 밖으로 나가지 않도록 위치를 보정합니다.
    """
    ox = target_x
    oy = target_y

    if ox + window_width > screen_x + screen_width:
        # 오른쪽 화면 밖으로 나가면 왼쪽으로 배치
        ox = ox - window_width * 2 - 32  # margin * 2

    # 그래도 왼쪽 화면 밖이거나 아예 모니터를 벗어나면 우하단 내부로 강제
    if ox < screen_x or ox + window_width > screen_x + screen_width:
        ox = screen_x + screen_width - window_width - 16
        oy = screen_y + screen_height - window_height - 16

    # 최종 안전 범위 클램핑
    ox = max(screen_x, min(ox, screen_x + screen_width - window_width))
    oy = max(screen_y, min(oy, screen_y + screen_height - window_height))

    return ox, oy
