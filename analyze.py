import os
import re

def analyze_codebase(root_dir):
    stats = []
    for dirpath, _, filenames in os.walk(root_dir):
        if 'target' in dirpath or '.git' in dirpath:
            continue
        for f in filenames:
            if f.endswith('.rs'):
                filepath = os.path.join(dirpath, f)
                with open(filepath, 'r', encoding='utf-8') as file:
                    content = file.read()
                    lines = content.split('\n')
                    num_lines = len(lines)
                    unwraps = len(re.findall(r'\.unwrap\(\)', content))
                    expects = len(re.findall(r'\.expect\(', content))
                    clones = len(re.findall(r'\.clone\(\)', content))
                    
                    func_lens = []
                    in_func = False
                    func_len = 0
                    for line in lines:
                        if re.match(r'^\s*(pub\s+)?(async\s+)?fn\s+', line):
                            in_func = True
                            func_len = 0
                        if in_func:
                            func_len += 1
                            if line.strip() == '}':
                                in_func = False
                                func_lens.append(func_len)
                    max_func_len = max(func_lens) if func_lens else 0
                    
                    stats.append({
                        'file': os.path.relpath(filepath, root_dir),
                        'lines': num_lines,
                        'unwraps': unwraps,
                        'expects': expects,
                        'clones': clones,
                        'max_func_len': max_func_len
                    })
    return stats

if __name__ == '__main__':
    stats = analyze_codebase('rust')
    print("--- Top 5 Largest Files ---")
    for s in sorted(stats, key=lambda x: x['lines'], reverse=True)[:5]:
        print(f"{s['file']:<40} {s['lines']} lines (max func {s['max_func_len']})")

    print("\n--- Files with most Unwraps + Expects ---")
    for s in sorted(stats, key=lambda x: x['unwraps'] + x['expects'], reverse=True)[:5]:
        print(f"{s['file']:<40} {s['unwraps']} unwraps, {s['expects']} expects")

    print("\n--- Files with most Clones ---")
    for s in sorted(stats, key=lambda x: x['clones'], reverse=True)[:5]:
        print(f"{s['file']:<40} {s['clones']} clones")
