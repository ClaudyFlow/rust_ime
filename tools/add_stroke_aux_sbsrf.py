import json
import os

# 1. 笔画映射 (sbsrf -> 标准数字 1-5)
# e:横(1), i:竖(2), u:撇(3), o:捺(4), a:折(5)
SB_MAP = {'e': 1, 'i': 2, 'u': 3, 'o': 4, 'a': 5}

# 2. 你的 5x5 键盘矩阵 (首笔区 x 次笔列)
# 横(1): Q W E R T
# 竖(2): A S D F G
# 撇(3): Z X C V B
# 捺(4): Y U I O P
# 折(5): H J K L N (M 不用)
MATRIX = {
    1: {1: 'Q', 2: 'W', 3: 'E', 4: 'R', 5: 'T'},
    2: {1: 'A', 2: 'S', 3: 'D', 4: 'F', 5: 'G'},
    3: {1: 'Z', 2: 'X', 3: 'C', 4: 'V', 5: 'B'},
    4: {1: 'Y', 2: 'U', 3: 'I', 4: 'O', 5: 'P'},
    5: {1: 'H', 2: 'J', 3: 'K', 4: 'L', 5: 'N'}
}

def load_sbsrf_data():
    file_path = 'reference/sbsrf/sbxlm/bihua.dict.yaml'
    char_map = {}
    with open(file_path, 'r', encoding='utf-8') as f:
        in_data = False
        for line in f:
            line = line.strip()
            if line == '...': in_data = True; continue
            if not in_data or not line or line.startswith('#'): continue
            parts = line.split()
            if len(parts) >= 2:
                char = parts[0]
                code = parts[1]
                
                # 获取前两笔
                s1 = SB_MAP.get(code[0])
                s2 = SB_MAP.get(code[1]) if len(code) >= 2 else 1
                
                # 获取末两笔
                if len(code) >= 4:
                    s3 = SB_MAP.get(code[-2])
                    s4 = SB_MAP.get(code[-1])
                elif len(code) == 3:
                    s3 = SB_MAP.get(code[-1])
                    s4 = 1 # 补横
                else:
                    s3 = 1
                    s4 = 1
                
                if s1 and s2 and s3 and s4:
                    char_map[char] = MATRIX[s1][s2] + MATRIX[s3][s4]
    return char_map

def process_file(file_path, stroke_map):
    print(f"Updating {file_path} with stroke aux codes...")
    if not os.path.exists(file_path): return
    with open(file_path, 'r', encoding='utf-8') as f:
        data = json.load(f)
    
    count = 0
    for key in data:
        for entry in data[key]:
            char = entry.get('char')
            if char in stroke_map:
                entry['stroke_aux'] = stroke_map[char]
                count += 1
    
    with open(file_path, 'w', encoding='utf-8') as f:
        json.dump(data, f, ensure_ascii=False, indent=2)
    print(f"Updated {count} characters in {file_path}.")

if __name__ == "__main__":
    stroke_map = load_sbsrf_data()
    files = [
        "dicts/chinese/chars/chars.json",
        "dicts/chinese/chars/level2.json",
        "dicts/chinese/chars/level3.json"
    ]
    for f in files:
        process_file(f, stroke_map)
