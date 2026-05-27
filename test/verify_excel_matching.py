import os
import sys
import json
import csv
import time
import urllib.request
from collections import defaultdict

CSV_CACHE_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), ".csv_cache")
CSV_CACHE_TTL = 60 * 60 * 24  # 24시간

DIFF_ALIASES = {
    "NORMAL": "NM",
    "HARD": "HD",
    "MAXIMUM": "MX",
    "SC": "SC",
    "NM": "NM",
    "HD": "HD",
    "MX": "MX",
}

def normalize_diff(diff):
    """표준 난이도(NM/HD/MX/SC)만 인식하고, DPC/FX/REDESIGN 등 특수 난이도는 None을 반환한다."""
    return DIFF_ALIASES.get(diff.strip().upper())

def fetch_csv(mode, gid, sheet_id):
    """CSV를 다운로드하되, 24시간 내 캐시가 있으면 캐시를 사용한다."""
    os.makedirs(CSV_CACHE_DIR, exist_ok=True)
    cache_path = os.path.join(CSV_CACHE_DIR, f"{mode}.csv")

    if os.path.exists(cache_path):
        age = time.time() - os.path.getmtime(cache_path)
        if age < CSV_CACHE_TTL:
            print(f"Using cached {mode} sheet CSV (age: {int(age // 60)}m)")
            with open(cache_path, encoding='utf-8') as f:
                return f.read()

    print(f"Downloading {mode} sheet CSV...")
    url = f"https://docs.google.com/spreadsheets/d/{sheet_id}/gviz/tq?tqx=out:csv&gid={gid}"
    req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
    with urllib.request.urlopen(req, timeout=15) as response:
        content = response.read().decode('utf-8')
    with open(cache_path, 'w', encoding='utf-8') as f:
        f.write(content)
    return content

# Simple implementation of normalized Damerau-Levenshtein distance in Python
def normalized_damerau_levenshtein(s1, s2):
    if s1 == s2:
        return 1.0
    len1 = len(s1)
    len2 = len(s2)
    if len1 == 0 or len2 == 0:
        return 0.0

    # Max distance
    max_dist = max(len1, len2)
    
    # Standard Damerau-Levenshtein distance
    d = {}
    for i in range(-1, len1 + 1):
        d[(i, -1)] = i + 1
    for j in range(-1, len2 + 1):
        d[(-1, j)] = j + 1

    for i in range(len1):
        for j in range(len2):
            cost = 0 if s1[i] == s2[j] else 1
            d[(i, j)] = min(
                d[(i - 1, j)] + 1,        # deletion
                d[(i, j - 1)] + 1,        # insertion
                d[(i - 1, j - 1)] + cost,  # substitution
            )
            if i > 0 and j > 0 and s1[i] == s2[j - 1] and s1[i - 1] == s2[j]:
                d[(i, j)] = min(d[(i, j)], d[(i - 2, j - 2)] + cost) # transposition

    dist = d[(len1 - 1, len2 - 1)]
    return 1.0 - (dist / max_dist)

def normalize_text(text):
    return text.lower().replace(" ", "").replace("\t", "").replace("\r", "").replace("\n", "").replace("腦", "뇌").replace("뇌", "脳").replace("擊", "격").replace("격", "撃").replace("腦", "脳").replace("擊", "撃")

def category_matches_dlc(category, dlc_code):
    cat = normalize_text(category)
    dlc = normalize_text(dlc_code)
    if dlc in cat or cat in dlc:
        return True
    
    mappings = {
        "respect/v": ["rv", "r"],
        "respect": ["rv", "r"],
        "emotional.s": ["es"],
        "emotionals.": ["es"],
        "vextension1": ["ve"],
        "vextension": ["ve"],
        "trilogy": ["tr"],
        "blacksquare": ["bs"],
        "clazziquai": ["ce"],
        "technika3": ["t3"],
        "technika2": ["t2"],
        "technika1": ["t1"],
        "technika": ["t1"],
        "portable3": ["pli3", "p3"],
        "pli3": ["pli3", "p3"],
        "portable2": ["pli2", "p2"],
        "pli": ["pli2", "p2"],
        "pli(상)": ["pli2", "p2"],
        "pli(하)": ["pli2", "p2"],
        "pli(下)": ["pli2", "p2"],
        "portable1": ["pli1", "p1"],
        "pli1": ["pli1", "p1"],
        "vextension2": ["ve2"],
        "vextension3": ["ve3"],
        "ez2on": ["ez2"],
        "vextension4": ["ve4"],
        "vextension5": ["ve5"],
        "vliberty": ["vl"],
        "vliberty2": ["vl2"],
        "vliberty3": ["vl3"],
        "vliberty4": ["vl4"],
    }
    
    if cat in mappings:
        return dlc in mappings[cat]
    return False

def find_best_match(songs, title, mode, diff, level, category, note):
    if not songs:
        return None
    
    # Try full title match first
    song = find_best_match_internal(songs, title, mode, diff, level, category, note)
    if song:
        return song
        
    # Try splitting by '/' for composite titles (DPC)
    if '/' in title:
        first_part = title.split('/')[0]
        song = find_best_match_internal(songs, first_part, mode, diff, level, category, note)
        if song:
            return song
            
    return None

def find_best_match_internal(songs, title, mode, diff, level, category, note):
    query_norm = normalize_text(title)
    if not query_norm:
        return None
    
    best_song = None
    best_score = -1000.0
    
    for song in songs:
        song_name_norm = normalize_text(song.get("name", ""))
        score = 0.0
        
        # 1. Title match
        if query_norm == song_name_norm:
            score += 100.0
        elif (query_norm.startswith(song_name_norm) or song_name_norm.startswith(query_norm)) and len(query_norm) >= 5 and len(song_name_norm) >= 5:
            score += 80.0
        else:
            dist = normalized_damerau_levenshtein(query_norm, song_name_norm)
            if dist >= 0.8:
                score += 50.0
            else:
                continue
        
        # 2. Pattern & Level check
        patterns = song.get("patterns", {})
        modes = patterns.get(mode, {})
        if diff in modes:
            p_info = modes[diff]
            score += 50.0
            if level is not None:
                p_level = p_info.get("level")
                if p_level == level:
                    score += 100.0
                else:
                    score -= 50.0
        
        # 3. Category / DLC match
        dlc_code = song.get("dlcCode", "")
        if category_matches_dlc(category, dlc_code):
            score += 80.0
            
        # 4. Composer in note check
        note_lower = note.lower()
        comp_lower = song.get("composer", "").lower()
        if note_lower and comp_lower:
            if comp_lower in note_lower or note_lower in comp_lower:
                score += 150.0
                
        if score > best_score:
            best_score = score
            best_song = song
            
    return best_song

def pattern_meta_value(mode, row_dict):
    raw_gold = row_dict.get("황배 여부") or row_dict.get("황배여부") or ""
    if not raw_gold:
        gold = ""
    elif "[H]" in raw_gold:
        gold = "핲랜"
    elif "[M]" in raw_gold:
        gold = "맥랜"
    else:
        gold = "랜덤"

    note = clean_cell_value(row_dict.get("비고") or row_dict.get("Note") or "")
    keypart = False

    if mode == "8B":
        raw_keypart = clean_cell_value(row_dict.get("키파트 위주") or row_dict.get("키파트위주") or "")
        if raw_keypart:
            keypart = True

    raw_assist = clean_cell_value(row_dict.get("보조 키 여부") or row_dict.get("보조키여부") or "")
    if "❌" in raw_assist:
        assist_key = "사용"
    elif "⚠️" in raw_assist or raw_assist.startswith("⚠"):
        assist_key = "주의"
    elif "✅" in raw_assist:
        assist_key = "미사용"
    else:
        assist_key = raw_assist

    return {
        "gold": gold,
        "note": note,
        "keypart": keypart,
        "assist_key": assist_key,
    }

import datetime

def clean_cell_value(val):
    if val is None:
        return ""
    if isinstance(val, datetime.time):
        return f"{val.hour}:{val.minute:02d}"
    return str(val).strip()

def main():
    # Find project root dynamically relative to this script's location
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    songs_path = os.path.join(root, "cache", "songs.json")
    meta_path = os.path.join(root, "cache", "pattern_meta.json")

    if not os.path.exists(songs_path) or not os.path.exists(meta_path):
        print("Missing required files to verify.")
        sys.exit(1)

    with open(songs_path, 'r', encoding='utf-8') as f:
        songs = json.load(f)
    with open(meta_path, 'r', encoding='utf-8') as f:
        meta_raw = json.load(f)
    # JSON Array → (song_id, mode, diff) 튜플 인덱스로 변환
    meta_cache = {
        (entry["song_id"], entry["mode"], entry["diff"]): entry
        for entry in meta_raw
    }

    sheet_id = "1ks1dwJyNjkAXYtQ_6UZIeNOCGOmhf2jMbakpTcJm9rw"
    gids = {
        "4B": "979055934",
        "5B": "112529029",
        "6B": "2010625608",
        "8B": "1833696991"
    }
    
    total_checked = 0
    failures = []
    
    # Keep track of duplicate song resolutions for logging
    duplicates_resolved = defaultdict(list)

    for mode, gid in gids.items():
        try:
            csv_content = fetch_csv(mode, gid, sheet_id)
        except Exception as e:
            print(f"Failed to fetch {mode} sheet: {e}")
            sys.exit(1)

        reader = csv.reader(csv_content.splitlines())
        rows = list(reader)
        if not rows:
            print(f"Empty data for {mode}")
            sys.exit(1)
            
        headers = [h.strip() for h in rows[0]]
        for row in rows[1:]:
            if not row or len(row) < 3 or not row[1]:
                continue
                
            row_dict = dict(zip(headers, row))
            title = clean_cell_value(row_dict.get("곡명"))
            diff = clean_cell_value(row_dict.get("난이도"))
            
            # Check if this row actually contains pattern meta
            expected_meta = pattern_meta_value(mode, row_dict)
            has_value = any(v for k, v in expected_meta.items() if v != "" and v is not False)
            if not has_value:
                continue
                
            level_val = row_dict.get("레벨")
            level = None
            if level_val is not None:
                try:
                    level = int(float(level_val))
                except ValueError:
                    level = None
            category = clean_cell_value(row_dict.get("카테고리"))
            note = clean_cell_value(row_dict.get("비고"))
            
            # Find matching song using Python's implementation of find_best_match
            matched_song = find_best_match(songs, title, mode, diff, level, category, note)
            if not matched_song:
                failures.append(f"{mode} {title} {diff} (Lv {level}): Song match failed in DB")
                continue
                
            song_id = str(matched_song["title"])
            composer = matched_song["composer"]

            norm_diff = normalize_diff(diff)
            if norm_diff is None:
                # DPC/FX/REDESIGN 등 특수 난이도 — Rust와 동일하게 skip
                continue

            cache_key = (song_id, mode, norm_diff)

            if cache_key not in meta_cache:
                failures.append(f"{mode} {title} {diff} (Lv {level}) [song_id={song_id}]: Not found in pattern_meta.json")
                continue

            cache_val = meta_cache[cache_key]
            
            # Check fields
            mismatches = []
            for field in ["gold", "note", "keypart", "assist_key"]:
                exp_v = expected_meta[field]
                cac_v = cache_val.get(field)
                
                # Handle boolean/string conversions if any
                if field == "keypart":
                    if bool(exp_v) != bool(cac_v):
                        mismatches.append(f"{field}: expected={exp_v}, cache={cac_v}")
                else:
                    exp_s = str(exp_v or "").strip()
                    cac_s = str(cac_v or "").strip()
                    if exp_s != cac_s:
                        mismatches.append(f"{field}: expected='{exp_s}', cache='{cac_s}'")
                        
            if mismatches:
                failures.append(f"{mode} {title} {diff} (Lv {level}) [song_id={song_id}]: Mismatches: {', '.join(mismatches)}")
            else:
                total_checked += 1
                # Log duplicates resolution for inspection
                if title in ["Alone", "Urban Night", "STOP", "Right Back", "Showdown", "Voyage"]:
                    duplicates_resolved[title].append(f"{mode} {diff} => song_id {song_id} ({composer})")

    print("\n--- Duplicate Song Match Resolutions ---")
    for title, matches in duplicates_resolved.items():
        print(f"Song: {title}")
        for match in sorted(set(matches)):
            print(f"  {match}")
            
    print(f"\nVerification finished. Total patterns checked: {total_checked}")
    if failures:
        print(f"Failures found: {len(failures)}")
        for fail in failures[:20]:
            print(f"  [FAIL] {fail}")
        if len(failures) > 20:
            print(f"  ... and {len(failures) - 20} more")
        sys.exit(1)
    else:
        print("SUCCESS: 100% matched keys and field values correspond perfectly to the Excel difficulty table!")
        sys.exit(0)

if __name__ == '__main__':
    main()
