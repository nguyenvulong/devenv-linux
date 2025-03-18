# Thiết Lập Môi Trường Phát Triển Linux

Một script đơn giản để thiết lập IDE trên Linux, cài đặt phiên bản mới nhất của các tools phổ biến từ GitHub vào `$HOME/.local/`.

## Tổng Quan

Script này cài đặt và cấu hình các tools sau:

- **Tiện Ích Shell**: fzf, ripgrep, fd, tmux, bat, eza
- **Ngôn Ngữ Lập Trình**: Rust, Node.js (qua nvm), Go
- **Công Cụ Phát Triển**: Neovim (với LazyVim), Nushell
- **Trình Quản Lý Gói**: uv (Python)

### Demo
[![asciicast](https://asciinema.org/a/708631.svg)](https://asciinema.org/a/708631)

### Tại sao mình chọn `$HOME/.local/`?

- Không cần quyền `sudo` để cập nhật
- Luôn nhận được các bản phát hành mới nhất từ GitHub
- Tránh xung đột với trình quản lý gói của hệ thống
- Đồng bộ trên các bản phân phối Linux khác nhau

## Sử Dụng

#### Clone từ Github
```bash
git clone https://github.com/nguyenvulong/devenv-linux.git
```
#### Chạy cài đặt
```bash
cd devenv-linux
chmod +x install.sh
./install.sh
```
#### Sau khi cài đặt
```
source $HOME/.bashrc
nvim # Chờ hoàn tất cài đặt plugin
```
Để thoát Neovim: nhấn `<space> q q`

## Lưu ý
- Đã thử nghiệm trên: Debian, Ubuntu, Fedora, CentOS Stream, và Arch Linux
- Phải chạy từ `bash` shell
- Cần quyền `sudo` để cài đặt `build essentials`, `tmux`, và `nushell`
- Có thể hơi ngợp cho người dùng mới
- Một số tools có thể dư thừa với nhu cầu của bạn

## Kế hoạch
- Hỗ trợ các bản phân phối của linux khác
- Cá nhân hóa cài đặt
- Trải niệm trên Docker

## Bản quyền
MIT
