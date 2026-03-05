import json
import os

def generate_stroke_words():
    char_dict_path = 'dicts/stroke/words/stroke_char.json'
    words_dict_path = 'dicts/chinese/words/words.json'
    output_path = 'dicts/stroke/words/stroke_words.json'
    
    # 1. 建立 汉字 -> 编码 的反向索引
    print("正在建立汉字编码索引...")
    char_to_code = {}
    if not os.path.exists(char_dict_path):
        print("错误: 未找到 stroke_char.json")
        return
        
    with open(char_dict_path, 'r', encoding='utf-8') as f:
        stroke_dict = json.load(f)
        for code, entries in stroke_dict.items():
            for entry in entries:
                char = entry['char']
                # 如果一个字有多个编码（虽然少见），保留最短的或第一个
                if char not in char_to_code or len(code) < len(char_to_code[char]):
                    char_to_code[char] = code

    # 2. 读取拼音词库并转换
    print("正在合成笔画组词词库...")
    stroke_words = {}
    if not os.path.exists(words_dict_path):
        print("错误: 未找到 words.json")
        return

    with open(words_dict_path, 'r', encoding='utf-8') as f:
        words_data = json.load(f)
        
    count = 0
    for py, entries in words_data.items():
        for entry in entries:
            word = entry['char']
            weight = entry.get('weight', 1)
            trad = entry.get('trad', word)
            
            # 只处理 2 字及以上的词
            if len(word) < 2: continue
            
            # 获取每个字的编码
            codes = []
            valid = True
            for char in word:
                if char in char_to_code:
                    codes.append(char_to_code[char])
                else:
                    valid = False
                    break
            
            if not valid: continue
            
            # 应用 4 键组词规则
            final_code = ""
            if len(word) == 2:
                # 2字词: 1前2 + 2前2
                c1 = codes[0]
                c2 = codes[1]
                final_code = c1[:2] + c2[:2]
            elif len(word) == 3:
                # 3字词: 1首 + 2首 + 3前2
                final_code = codes[0][0] + codes[1][0] + codes[2][:2]
            else:
                # 4字及以上: 1首 + 2首 + 3首 + 末首
                final_code = codes[0][0] + codes[1][0] + codes[2][0] + codes[-1][0]
            
            if final_code:
                if final_code not in stroke_words:
                    stroke_words[final_code] = []
                
                stroke_words[final_code].append({
                    "char": word,
                    "weight": weight,
                    "trad": trad
                })
                count += 1

    # 3. 排序并写回
    print(f"正在保存 {count} 条词组记录...")
    for code in stroke_words:
        stroke_words[code].sort(key=lambda x: x['weight'], reverse=True)
        
    with open(output_path, 'w', encoding='utf-8') as f:
        json.dump(stroke_words, f, ensure_ascii=False, indent=2)
        
    print("笔画组词词库合成完成！")

if __name__ == "__main__":
    generate_stroke_words()
