# Thiết Lập Môi Trường Phát Triển Linux

Trình cài đặt TUI tương tác giúp thiết lập môi trường phát triển trên Linux.  
Viết bằng Rust + [Ratatui](https://github.com/ratatui/ratatui). Quản lý công cụ bằng [mise](https://mise.jdx.dev).

## Những Gì Được Cài Đặt

| Nhóm | Công cụ |
|---|---|
| Tiện ích shell | fzf, ripgrep, fd, bat, eza, glow, jaq |
| Terminal | tmux (+ oh-my-tmux) |
| Ngôn ngữ | Rust, Node.js, Go, Python (uv) |
| Trình soạn thảo | Neovim (LazyVim) |
| Shell | Fish, Nushell |

## Bắt Đầu Nhanh

```bash
git clone https://github.com/nguyenvulong/devenv-linux.git
cd devenv-linux
bash install.sh
```

`install.sh` sẽ:
1. Cài build essentials & Rust (nếu chưa có)
2. Biên dịch trình cài đặt TUI
3. Mở menu tương tác — chọn công cụ bạn muốn rồi nhấn **Enter**

> **Yêu cầu:** `bash`, `curl`, `git` và quyền `sudo` (để cài system packages & tmux).

## Sau Khi Cài Đặt

Tải lại shell:

```bash
source ~/.bashrc                       # bash
source ~/.config/fish/config.fish      # fish
```

Mở Neovim một lần để hoàn tất cài đặt plugin:

```bash
nvim
# Chờ plugin cài xong, rồi nhấn <Space>qq để thoát
```

## Chế Độ Tự Động (Không TUI)

Bỏ qua giao diện và cài tất cả tự động:

```bash
bash install.sh --all
# hoặc
CI=true bash install.sh
```

## Các Distro Được Hỗ Trợ

Ubuntu 24.04 · Debian 13 · Fedora 43 · Arch Linux

## Bản Quyền

MIT
