import os
import re


def find_steam_path():
    # 기본 경로들 (우선순위)
    possible_paths = [
        r"C:\Program Files (x86)\Steam",
        r"C:\Program Files\Steam",
    ]

    for path in possible_paths:
        if os.path.exists(path):
            return path

    # 레지스트리 fallback
    try:
        import winreg
        key = winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Software\Valve\Steam")
        steam_path, _ = winreg.QueryValueEx(key, "SteamPath")
        return steam_path
    except:
        pass

    return None


def get_most_recent_steam_id():
    steam_path = find_steam_path()
    if not steam_path:
        return None

    vdf_path = os.path.join(steam_path, "config", "loginusers.vdf")

    if not os.path.exists(vdf_path):
        return None

    with open(vdf_path, "r", encoding="utf-8") as f:
        content = f.read()

    # SteamID64 블록 찾기
    pattern = re.compile(r'"(\d+)"\s*{[^}]*"MostRecent"\s*"1"', re.MULTILINE)
    match = pattern.search(content)

    if match:
        return match.group(1)

    return None


def mask_steam_id(steam_id: str | None) -> str:
    if not steam_id:
        return "(none)"
    steam_id = str(steam_id).strip()
    if len(steam_id) <= 8:
        return "***"
    return f"{steam_id[:4]}...{steam_id[-4:]}"


if __name__ == "__main__":
    steam_id = get_most_recent_steam_id()
    print("Most recent SteamID64:", mask_steam_id(steam_id))
