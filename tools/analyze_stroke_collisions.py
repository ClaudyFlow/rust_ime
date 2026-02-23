import json
import os
from collections import defaultdict

def analyze_collisions():
    char_files = [
        "dicts/chinese/chars/chars.json",
        "dicts/chinese/chars/level2.json",
        "dicts/chinese/chars/level3.json"
    ]
    word_files = [
        "dicts/chinese/words/words.json"
    ]

    # key: (pinyin, stroke_aux), value: list of chars/words
    collision_map = defaultdict(list)
    total_entries = 0

    print("Loading dictionaries and analyzing collisions...")

    # 1. 单字
    for file in char_files:
        if not os.path.exists(file): continue
        with open(file, 'r', encoding='utf-8') as f:
            data = json.load(f)
            for pinyin, entries in data.items():
                for entry in entries:
                    char = entry.get('char')
                    stroke = entry.get('stroke_aux', '')
                    if char and stroke:
                        collision_map[(pinyin, stroke)].append(char)
                        total_entries += 1

    # 2. 二字词
    for file in word_files:
        if not os.path.exists(file): continue
        with open(file, 'r', encoding='utf-8') as f:
            data = json.load(f)
            for pinyin, entries in data.items():
                for entry in entries:
                    word = entry.get('char')
                    stroke = entry.get('stroke_aux', '')
                    if word and len(word) == 2 and stroke:
                        collision_map[(pinyin, stroke)].append(word)
                        total_entries += 1

    # 3. 统计
    collisions = {k: v for k, v in collision_map.items() if len(v) > 1}
    unique_combos = len(collision_map)
    collision_count = len(collisions)
    
    print("\n" + "="*50)
    print("Stroke Aux Collision Analysis")
    print("="*50)
    print(f"Total entries: {total_entries}")
    print(f"Unique (Pinyin + Stroke) combos: {unique_combos}")
    print(f"Combinations with collisions: {collision_count}")
    print(f"Overall collision rate: {(collision_count / unique_combos * 100):.2f}%")
    
    sorted_collisions = sorted(collisions.items(), key=lambda x: len(x[1]), reverse=True)

    print("\n[Top 20 Collisions]:")
    print(f"{'Pinyin':<15} {'Stroke':<8} {'Count':<6} {'Candidates'}")
    print("-" * 70)
    for (pinyin, stroke), candidates in sorted_collisions[:20]:
        cand_str = ", ".join(candidates[:8])
        if len(candidates) > 8: cand_str += "..."
        print(f"{pinyin:<15} {stroke:<8} {len(candidates):<6} {cand_str}")

    report_path = "stroke_collision_report.txt"
    with open(report_path, 'w', encoding='utf-8') as f:
        for (pinyin, stroke), candidates in sorted_collisions:
            f.write(f"{pinyin} [{stroke}] -> {', '.join(candidates)}\n")
    
    print(f"\nDetailed report saved to: {report_path}")

if __name__ == "__main__":
    analyze_collisions()
