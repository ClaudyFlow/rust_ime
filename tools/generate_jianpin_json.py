import json
import os

def load_syllables(path):
    with open(path, 'r', encoding='utf-8') as f:
        return set(line.strip() for line in f if line.strip())

def split_pinyin(pinyin, syllables):
    res = []
    i = 0
    while i < len(pinyin):
        found = False
        # Try longest match first (max syllable length usually 6)
        for length in range(6, 0, -1):
            if i + length <= len(pinyin):
                sub = pinyin[i:i+length]
                if sub in syllables:
                    res.append(sub)
                    i += length
                    found = True
                    break
        if not found:
            # Fallback for unknown syllables (e.g. single chars)
            res.append(pinyin[i])
            i += 1
    return res

def generate_jianpin():
    words_path = 'dicts/chinese/words.json'
    syllables_path = 'dicts/chinese/syllables.txt'
    output_path = 'dicts/chinese/words_jianpin.json'
    
    if not os.path.exists(words_path) or not os.path.exists(syllables_path):
        print(f"Error: {words_path} or {syllables_path} not found.")
        return

    print("Loading syllables...")
    syllables = load_syllables(syllables_path)
    
    print("Loading words...")
    with open(words_path, 'r', encoding='utf-8') as f:
        words_data = json.load(f)
        
    jianpin_map = {}
    
    print("Generating jianpin...")
    count = 0
    for pinyin, entries in words_data.items():
        syls = split_pinyin(pinyin, syllables)
        jianpin = "".join([s[0] for s in syls])
        
        # Only add if jianpin is different from full pinyin (and valid)
        if len(jianpin) > 1 and jianpin != pinyin:
            if jianpin not in jianpin_map:
                jianpin_map[jianpin] = []
            
            # words_data values can be list of strings or objects
            # We want to preserve the structure
            if isinstance(entries, list):
                for entry in entries:
                    # Avoid duplicates in the target list
                    if entry not in jianpin_map[jianpin]:
                        jianpin_map[jianpin].append(entry)
            else:
                print(f"Warning: Unexpected format for {pinyin}")
                
        count += 1
        if count % 10000 == 0:
            print(f"Processed {count} entries...")

    print(f"Writing to {output_path}...")
    with open(output_path, 'w', encoding='utf-8') as f:
        json.dump(jianpin_map, f, ensure_ascii=False, indent=2)
        
    print("Done!")

if __name__ == "__main__":
    generate_jianpin()
