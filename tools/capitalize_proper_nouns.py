import os

def capitalize_pinyin(pinyin_str):
    syllables = pinyin_str.split()
    capitalized = [s.capitalize() for s in syllables]
    return " ".join(capitalized)

def process_directory(base_path):
    if not os.path.exists(base_path):
        print(f"跳过不存在的目录: {base_path}")
        return

    print(f"正在处理目录: {base_path}")
    for root, dirs, files in os.walk(base_path):
        for file in files:
            if file.endswith('.txt'):
                file_path = os.path.join(root, file)
                print(f"  处理文件: {file}")
                
                new_lines = []
                with open(file_path, 'r', encoding='utf-8') as f:
                    for line in f:
                        if not line.strip() or line.startswith('#'):
                            new_lines.append(line)
                            continue
                        
                        parts = line.split('\t')
                        if len(parts) >= 2:
                            word = parts[0]
                            pinyin = parts[1].strip()
                            rest = parts[2:] # 保留可能存在的权重或其他列
                            
                            new_pinyin = capitalize_pinyin(pinyin)
                            
                            new_line = f"{word}\t{new_pinyin}"
                            for r in rest:
                                new_line += f"\t{r.strip()}"
                            new_line += "\n"
                            new_lines.append(new_line)
                        else:
                            new_lines.append(line)
                
                with open(file_path, 'w', encoding='utf-8') as f:
                    f.writelines(new_lines)

if __name__ == "__main__":
    proper_noun_dirs = [
        "dicts/chinese/words/capital_word/人名",
        "dicts/chinese/words/capital_word/地名"
    ]
    
    for target_dir in proper_noun_dirs:
        process_directory(target_dir)
    
    print("\n处理完成！")
