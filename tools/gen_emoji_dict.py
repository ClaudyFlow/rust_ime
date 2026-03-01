import json
import os

# 基础 Emoji 映射数据
EMOJI_DATA = {
    "xiao": ["😄", "😆", "😅", "😂", "🤣", "😊", "😇"],
    "ku": ["😭", "😢", "☹️", "😩", "😫"],
    "nu": ["😡", "😠", "🤬", "😤"],
    "xin": ["❤️", "🧡", "💛", "💚", "💙", "💜", "🖤", "💕", "💞"],
    "zan": ["👍", "👏", "🙌", "💪"],
    "huo": ["🔥", "🌋"],
    "shui": ["💧", "💦", "🌊", "🛀"],
    "taiyang": ["☀️", "🌞"],
    "yue亮": ["🌙", "🌙", "🌛", "🌜"],
    "ok": ["👌"],
    "v": ["✌️"],
    "doge": ["🐶", "🐕"],
    "mao": ["🐱", "🐈"],
    "meigui": ["🌹"],
    "liwu": ["🎁"],
    "shengri": ["🎂", "🎈"],
    "ij": ["📱", "💻", "💻"],
    "diannao": ["💻", "🖥️"],
    "shouji": ["📱"],
    "qian": ["💰", "💵", "💴", "💶", "💷"],
    "kaixin": ["🥳", "😄"],
    "se": ["😍", "🤩"],
}

def generate_emoji_dict():
    output_dir = "dicts/emoji"
    if not os.path.exists(output_dir):
        os.makedirs(output_dir)
    
    dict_data = {}
    for pinyin, chars in EMOJI_DATA.items():
        entries = []
        for i, char in enumerate(chars):
            entries.append({
                "char": char,
                "trad": char, # Emoji 无需繁简转换
                "en": "Emoji",
                "category": "emoji",
                "weight": 1000 - (i * 10) # 越靠前的权重越高
            })
        dict_data[pinyin] = entries
    
    output_file = os.path.join(output_dir, "emoji.json")
    with open(output_file, "w", encoding="utf-8") as f:
        json.dump(dict_data, f, ensure_ascii=False, indent=2)
    
    print(f"Emoji dictionary generated at {output_file} with {len(EMOJI_DATA)} keys.")

if __name__ == "__main__":
    generate_emoji_dict()
