let config = null;

async function loadConfig() {
    const r = await fetch('/api/config');
    config = await r.json();
    return config;
}

async function saveConfig() {
    await fetch('/api/config', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(config)
    });
    showToast("已保存并应用");
}

function showToast(message) {
    const toast = document.getElementById('toast');
    if (toast) {
        toast.innerText = message;
        toast.style.display = 'block';
        setTimeout(() => toast.style.display = 'none', 2000);
    }
}

async function resetConfig() {
    if (confirm("确定要重置所有设置到默认值吗？")) {
        await fetch('/api/config/reset', { method: 'POST' });
        location.reload();
    }
}

function bindInput(id, section, property) {
    const el = document.getElementById(id);
    if (!el) return;

    let val;
    if (section && property) {
        val = config[section][property];
    } else if (section) {
        val = config[section][id];
    } else {
        val = config[id];
    }

    if (el.type === 'checkbox') {
        el.checked = !!val;
        el.onchange = () => {
            if (section && property) config[section][property] = el.checked;
            else if (section) config[section][id] = el.checked;
            else config[id] = el.checked;
        };
    } else {
        el.value = val !== undefined ? val : "";
        el.oninput = () => {
            const newVal = el.type === 'number' ? parseFloat(el.value) : el.value;
            if (section && property) config[section][property] = newVal;
            else if (section) config[section][id] = newVal;
            else config[id] = newVal;
        };
    }
}

function linkColor(id, section, property) {
    const txt = document.getElementById(id);
    const picker = document.getElementById(id + '_picker');
    if (!txt || !picker) return;

    txt.oninput = () => {
        picker.value = txt.value;
        if (section && property) config[section][property] = txt.value;
        else if (section) config[section][id] = txt.value;
    };
    picker.oninput = () => {
        txt.value = picker.value;
        if (section && property) config[section][property] = picker.value;
        else if (section) config[section][id] = picker.value;
    };
}

async function loadFonts(selectIds) {
    const font_r = await fetch('/api/fonts');
    const system_fonts = await font_r.json();
    const font_options = system_fonts.map(f => `<option value="${f.name}">${f.name}</option>`).join('');
    
    selectIds.forEach(id => {
        const select = document.getElementById(id);
        if (select) {
            const currentVal = select.getAttribute('data-value');
            select.innerHTML = `<option value="">默认系统字体</option>` + font_options;
            if (currentVal) select.value = currentVal;
        }
    });
}
