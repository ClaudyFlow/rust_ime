let config = null;

async function loadConfig() {
    const r = await fetch('/api/config');
    config = await r.json();
    return config;
}

async function saveConfig() {
    // 特别处理：显式同步所有已知复选框的状态，防止某些页面没调用 save() 导致的状态丢失
    const checkboxes = document.querySelectorAll('input[type="checkbox"]');
    checkboxes.forEach(cb => {
        const id = cb.id;
        // 尝试从 bindInput 建立的关系中查找所属 section
        // 这里采用简单的尝试策略，或者根据 appearance.html 的结构
        if (config.appearance && id in config.appearance) {
            config.appearance[id] = cb.checked;
        } else if (config.input && id in config.input) {
            config.input[id] = cb.checked;
        } else if (config.hotkeys && id in config.hotkeys) {
            config.hotkeys[id] = cb.checked;
        } else if (id in config) {
            config[id] = cb.checked;
        }
    });

    await fetch('/api/config', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(config)
    });
    showToast();
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
        const update = () => {
            const newVal = el.type === 'number' ? parseFloat(el.value) : el.value;
            if (section && property) config[section][property] = newVal;
            else if (section) config[section][id] = newVal;
            else config[id] = newVal;
        };
        el.oninput = update;
        if (el.tagName === 'SELECT') {
            el.onchange = update;
        }
    }
}

function linkColor(id, section, property) {
    const txt = document.getElementById(id);
    const picker = document.getElementById(id + '_picker');
    if (!txt || !picker) return;

    const toHex = (color) => {
        if (!color) return "#000000";
        if (color.startsWith('#')) return color.substring(0, 7);
        if (color.startsWith('rgba') || color.startsWith('rgb')) {
            const m = color.match(/\d+/g);
            if (m && m.length >= 3) {
                return "#" + m.slice(0, 3).map(x => parseInt(x).toString(16).padStart(2, '0')).join('');
            }
        }
        return "#000000";
    };

    // 初始化同步
    if (txt.value) {
        picker.value = toHex(txt.value);
    }

    txt.oninput = () => {
        picker.value = toHex(txt.value);
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

async function loadDictionaryViewer(filePath) {
    const tableBody = document.querySelector('#charsTable tbody');
    if (!tableBody) return;

    // 清空现有 DataTable
    if ($.fn.DataTable.isDataTable('#charsTable')) {
        $('#charsTable').DataTable().destroy();
    }

    tableBody.innerHTML = '<tr><td colspan="5" class="text-center py-5"><div class="spinner-border text-primary"></div></td></tr>';
    document.getElementById('fileInfo').innerText = '正在读取: ' + filePath;

    try {
        const url = filePath ? `/api/dictionary/chars?file=${encodeURIComponent(filePath)}` : '/api/dictionary/chars';
        const response = await fetch(url);
        const data = await response.json();
        
        let html = '';
        data.forEach(item => {
            html += `<tr class="pinyin-group-${item.group}">
                <td>${item.pinyin}</td>
                <td class="char-cell">${item.char}</td>
                <td><span class="aux-highlight-en">${item.en_aux}</span></td>
                <td><span class="aux-highlight-stroke">${item.stroke_aux}</span></td>
                <td>${item.en_meaning}</td>
            </tr>`;
        });
        
        tableBody.innerHTML = html;
        
        if (window.jQuery && $.fn.DataTable) {
            $('#charsTable').DataTable({
                pageLength: 25,
                language: {
                    search: "快速过滤：",
                    lengthMenu: "每页显示 _MENU_ 条",
                    info: "第 _START_ 到 _END_ 条，共 _TOTAL_ 条",
                    paginate: { first: "首页", last: "末页", next: "下一页", previous: "上一页" }
                },
                order: [[0, 'asc']]
            });
        }
    } catch (e) {
        tableBody.innerHTML = '<tr><td colspan="5" class="text-center text-danger py-5">数据加载失败: ' + e.message + '</td></tr>';
    }
}
