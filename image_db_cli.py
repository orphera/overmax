from __future__ import annotations

from pathlib import Path

import cv2

from image_db import ImageDB


SUPPORTED_EXTENSIONS = {".png", ".jpg", ".jpeg", ".bmp", ".webp"}


def _resolve_default_db_path() -> str:
    try:
        from settings import SETTINGS

        return str(SETTINGS["jacket_matcher"]["db_path"])
    except Exception:
        return "cache/image_index.db"


def _print_menu() -> None:
    print("\n=== ImageDB 관리자 ===")
    print("1) 테이블 조회")
    print("2) 단일 항목 조회(song_id)")
    print("3) 단일 이미지 추가")
    print("4) 폴더 일괄 추가")
    print("5) 단일 항목 삭제(song_id)")
    print("0) 종료")


def _input_int(prompt: str) -> int | None:
    raw = input(prompt).strip()
    if not raw:
        return None
    if not raw.isdigit():
        return None
    return int(raw)


def _input_song_id(prompt: str) -> str | None:
    song_id = input(prompt).strip()
    if not song_id:
        return None
    if not song_id.isdigit():
        return None
    return song_id


def _show_table(db: ImageDB) -> None:
    stats = db.get_stats()
    if stats is None:
        print("[CLI] 테이블 통계 조회 실패")
        return

    print(
        f"[CLI] total_rows={stats['total_rows']}, "
        f"distinct_song_ids={stats['distinct_song_ids']}"
    )

    limit = _input_int("표시할 개수(limit, 기본 20): ") or 20
    entries = db.list_entries(limit=limit, offset=0)
    if not entries:
        print("[CLI] 표시할 항목이 없습니다.")
        return

    print("[id] song_id (has_orb)")
    for e in entries:
        print(f"- {e['id']} {e['image_id']} (orb={e['has_orb']})")


def _show_entry(db: ImageDB) -> None:
    song_id = _input_song_id("조회할 song_id(숫자): ")
    if song_id is None:
        print("[CLI] 숫자 song_id를 입력하세요.")
        return

    entry = db.get_entry(song_id)
    if entry is None:
        print(f"[CLI] song_id={song_id} 항목 없음")
        return

    print("[CLI] 조회 결과")
    print(f"id={entry['id']}")
    print(f"song_id={entry['image_id']}")
    print(f"phash={entry['phash']}")
    print(f"dhash={entry['dhash']}")
    print(f"ahash={entry['ahash']}")
    print(f"hog_size={entry['hog_size']} bytes")
    print(f"has_orb={entry['has_orb']}, orb_size={entry['orb_size']} bytes")


def _add_single_image(db: ImageDB) -> None:
    song_id = input("song_id(숫자): ").strip()
    if not song_id or not song_id.isdigit():
        print("[CLI] song_id는 숫자 문자열이어야 합니다.")
        return

    path_raw = input("이미지 경로: ").strip().strip('"')
    if not path_raw:
        print("[CLI] 이미지 경로를 입력하세요.")
        return

    img_path = Path(path_raw)
    if not img_path.exists() or not img_path.is_file():
        print(f"[CLI] 파일이 존재하지 않습니다: {img_path}")
        return

    img = cv2.imread(str(img_path), cv2.IMREAD_UNCHANGED)
    if img is None:
        print(f"[CLI] 이미지 로드 실패: {img_path}")
        return

    ok = db.register(song_id, img)
    if ok:
        print(f"[CLI] 등록 성공: song_id={song_id}, path={img_path}")
    else:
        print(f"[CLI] 등록 실패: song_id={song_id}, path={img_path}")


def _add_directory_images(db: ImageDB) -> None:
    path_raw = input("폴더 경로: ").strip().strip('"')
    if not path_raw:
        print("[CLI] 폴더 경로를 입력하세요.")
        return

    root = Path(path_raw)
    if not root.exists() or not root.is_dir():
        print(f"[CLI] 폴더가 존재하지 않습니다: {root}")
        return

    files = [
        p for p in root.rglob("*")
        if p.is_file() and p.suffix.lower() in SUPPORTED_EXTENSIONS
    ]
    if not files:
        print("[CLI] 처리 가능한 이미지가 없습니다.")
        return

    success = 0
    fail = 0
    skip_non_numeric = 0

    for fp in files:
        song_id = fp.stem.strip()
        if not song_id.isdigit():
            skip_non_numeric += 1
            print(f"[SKIP] 숫자 song_id 아님: {fp.name}")
            continue

        img = cv2.imread(str(fp), cv2.IMREAD_UNCHANGED)
        if img is None:
            fail += 1
            print(f"[FAIL] 로드 실패: {fp}")
            continue

        if db.register(song_id, img):
            success += 1
            print(f"[ OK ] {fp.name} -> song_id={song_id}")
        else:
            fail += 1
            print(f"[FAIL] 등록 실패: {fp.name} -> song_id={song_id}")

    print(
        f"[CLI] 일괄 추가 완료: success={success}, fail={fail}, "
        f"skip_non_numeric={skip_non_numeric}, total_images={len(files)}"
    )


def _delete_single_entry(db: ImageDB) -> None:
    song_id = _input_song_id("삭제할 song_id(숫자): ")
    if song_id is None:
        print("[CLI] 숫자 song_id를 입력하세요.")
        return

    entry = db.get_entry(song_id)
    if entry is None:
        print(f"[CLI] song_id={song_id} 항목 없음")
        return

    confirm = input(
        f"정말 삭제할까요? song_id={entry['image_id']} (y/N): "
    ).strip().lower()
    if confirm != "y":
        print("[CLI] 삭제 취소")
        return

    ok = db.delete_entry(song_id)
    if ok:
        print(f"[CLI] 삭제 완료: song_id={song_id}")
    else:
        print(f"[CLI] 삭제 실패: song_id={song_id}")


def run_cli() -> None:
    db_path = _resolve_default_db_path()
    print(f"[CLI] DB 경로: {db_path}")

    db = ImageDB(db_path=db_path)
    if not db.initialize():
        print("[CLI] DB 초기화 실패")
        return
    db.load()

    while True:
        _print_menu()
        cmd = input("선택: ").strip()

        if cmd == "1":
            _show_table(db)
        elif cmd == "2":
            _show_entry(db)
        elif cmd == "3":
            _add_single_image(db)
        elif cmd == "4":
            _add_directory_images(db)
        elif cmd == "5":
            _delete_single_entry(db)
        elif cmd == "0":
            print("[CLI] 종료")
            break
        else:
            print("[CLI] 알 수 없는 메뉴입니다.")


if __name__ == "__main__":
    run_cli()
