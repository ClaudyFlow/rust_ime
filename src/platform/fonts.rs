use std::process::Command;

#[derive(serde::Serialize)]
pub struct FontInfo {
    pub name: String,
    pub path: String,
}

pub fn list_system_fonts() -> Vec<FontInfo> {
    #[cfg(target_os = "windows")]
    {
        list_fonts_windows()
    }
    #[cfg(target_os = "linux")]
    {
        list_fonts_linux()
    }
}

#[cfg(target_os = "windows")]
fn list_fonts_windows() -> Vec<FontInfo> {
    // 使用 PowerShell 获取注册表中的字体列表
    // HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts 存储了 名称 -> 文件名 的映射
    let script = r#"
    Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts' | 
    Select-Object -Property * -ExcludeProperty PSPath,PSParentPath,PSChildName,PSDrive,PSProvider | 
    Get-Member -MemberType NoteProperty | 
    ForEach-Object { 
        $name = $_.Name; 
        $file = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts').$($name);
        Write-Output "$name|$file" 
    }
    "#;

    let output = Command::new("powershell")
        .args(&["-NoProfile", "-Command", script])
        .output()
        .ok();

    let mut fonts = Vec::new();
    if let Some(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() >= 2 {
                let name = parts[0].trim().replace(" (TrueType)", "").to_string();
                let mut path = parts[1].trim().to_string();
                
                // 如果路径只是文件名，则补全 C:\Windows\Fonts
                if !path.contains('\\') && !path.contains('/') {
                    path = format!("C:\\Windows\\Fonts\\{}", path);
                }
                
                fonts.push(FontInfo { name, path });
            }
        }
    }
    // 排序
    fonts.sort_by(|a, b| a.name.cmp(&b.name));
    fonts
}

#[cfg(target_os = "linux")]
fn list_fonts_linux() -> Vec<FontInfo> {
    // 使用 fc-list : family file
    let output = Command::new("fc-list")
        .arg(":")
        .arg("family")
        .arg("file")
        .output()
        .ok();

    let mut fonts = Vec::new();
    if let Some(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            // 格式通常是: /path/to/font.ttf: Family Name,Other Name
            if let Some(idx) = line.find(": ") {
                let path = line[..idx].trim().to_string();
                let families = &line[idx+2..];
                // 可能有多个名称，取第一个
                let name = families.split(',').next().unwrap_or("Unknown").trim().to_string();
                
                if !name.is_empty() {
                    fonts.push(FontInfo { name, path });
                }
            }
        }
    }
    fonts.sort_by(|a, b| a.name.cmp(&b.name));
    fonts.dedup_by(|a, b| a.name == b.name); // 简单去重
    fonts
}