import json
import os

def convert():
    base_path = 'dicts/chinese/words/capital_word'
    if not os.path.exists(base_path):
        print("Directory not found!")
        return

    for root, dirs, files in os.walk(base_path):
        for file in files:
            if file.endswith('.txt'):
                txt_path = os.path.join(root, file)
                json_path = txt_path.replace('.txt', '.json')
                category = os.path.splitext(file)[0]
                
                print(f"Converting {txt_path} -> {json_path}")
                
                json_data = {}
                with open(txt_path, 'r', encoding='utf-8') as f:
                    for line in f:
                        if not line.strip() or line.startswith('#'):
                            continue
                        
                        parts = line.split('\t')
                        if len(parts) >= 2:
                            word = parts[0].strip()
                            pinyin_raw = parts[1].strip()
                            # 索引键：去掉空格
                            pinyin_key = pinyin_raw.replace(' ', '')
                            
                            if pinyin_key not in json_data:
                                json_data[pinyin_key] = []
                            
                            json_data[pinyin_key].append({
                                "char": word,
                                "en": pinyin_raw, # 提示即为原始拼音
                                "category": category
                            })
                
                with open(json_path, 'w', encoding='utf-8') as f:
                    json.dump(json_data, f, ensure_ascii=False, indent=2)
                
                # 删除原 TXT 文件
                os.remove(txt_path)

if __name__ == "__main__":
    convert()
    print("Conversion complete. TXT files replaced with JSON.")
