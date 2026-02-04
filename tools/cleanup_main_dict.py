import json
import os

def cleanup():
    words_path = 'dicts/chinese/words/words.json'
    phrases_path = 'dicts/chinese/words/phrases.json'
    
    if not os.path.exists(words_path):
        print("words.json not found!")
        return

    with open(words_path, 'r', encoding='utf-8') as f:
        words_data = json.load(f)

    new_words = {}
    phrases = {}

    print("Processing dictionary entries...")
    for pinyin, entries in words_data.items():
        for entry in entries:
            en = entry.get('en', '').strip()
            # 判断是否为多个单词 (包含空格)
            if ' ' in en:
                if pinyin not in phrases:
                    phrases[pinyin] = []
                phrases[pinyin].append(entry)
            else:
                # 单个单词或空：首字母大写
                if en:
                    entry['en'] = en.capitalize()
                if pinyin not in new_words:
                    new_words[pinyin] = []
                new_words[pinyin].append(entry)

    print(f"Saving {len(new_words)} entries to words.json")
    with open(words_path, 'w', encoding='utf-8') as f:
        json.dump(new_words, f, ensure_ascii=False, indent=2)

    print(f"Saving {len(phrases)} phrase entries to phrases.json")
    with open(phrases_path, 'w', encoding='utf-8') as f:
        json.dump(phrases, f, ensure_ascii=False, indent=2)

if __name__ == "__main__":
    cleanup()
