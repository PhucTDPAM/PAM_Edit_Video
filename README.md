<p align="center">
  <img src="assets/icons/pamedit_logo_256.png" alt="PAM_Edit Video" width="120" />
</p>

<h1 align="center">PAM_Edit Video</h1>

<p align="center">Trình dựng video chạy hoàn toàn trên máy của bạn: không watermark, không tài khoản, không tải video lên máy chủ.</p>

## Tính năng

- **Phụ đề tự động** bằng mô hình Whisper chạy offline, chỉnh kiểu chữ ngay trên timeline.
- **Chuyển văn bản thành giọng nói**, có nhân bản giọng từ một đoạn ghi âm ngắn.
- **Xoá phông nền** tự động hoặc tô tay.
- **Keyframe** cho vị trí, kích thước, góc xoay, độ mờ, âm lượng và thông số hiệu ứng, có trình chỉnh đường cong.
- **Hơn 170 hiệu ứng**, bộ lọc, chuyển cảnh và hiệu ứng chữ, xem trước trực tiếp.
- **Nhiều track, nhiều timeline**: cắt, trim, ripple, ghép, đóng băng khung hình, đổi tốc độ.
- **Làm sạch giọng nói**: lọc nhiễu, làm rõ giọng, cân bằng âm lượng.
- **Xuất video** H.264, HEVC, AV1, tối đa 4K 60fps, màu 10-bit.
- **Tự động hoá** qua JSON-RPC, gRPC, MCP và CLI (`pamedit-cli`).

## Bảo mật và kết nối mạng

So với mã gốc, bản này đã được chỉnh để không kết nối tới máy chủ của dự án gốc:

| Thành phần | Trạng thái |
| --- | --- |
| Tự cập nhật (Settings → Version) | **Đã tắt.** Ứng dụng không hỏi máy chủ nào về bản mới. |
| Tải mô hình AI | Tải thẳng từ nguồn gốc (HuggingFace, GitHub của tác giả mô hình), có kiểm tra SHA-256. Chỉ chạy khi bạn bấm tải trong Settings. |
| Telemetry, analytics | Không có. Nhật ký lỗi chỉ ghi ra file trên máy. |
| API cục bộ | Mặc định chỉ nghe trên `127.0.0.1` và chỉ chạy khi bạn bật. |

Mô hình chỉ cần tải một lần, sau đó mọi tính năng AI chạy offline.

## Build từ mã nguồn

Cần [Rust](https://rustup.rs) bản 1.93 trở lên. Xem chi tiết các thư viện hệ thống trong [CONTRIBUTING.md](CONTRIBUTING.md).

```sh
cd src
cargo build --release --bin pamedit
```

File chạy nằm ở `src/target/release/pamedit` (Windows: `pamedit.exe`).

Tài liệu kỹ thuật: [ARCHITECTURE.md](ARCHITECTURE.md) và [src/README.md](src/README.md).

## Giấy phép

PAM_Edit Video là bản sửa đổi của một phần mềm mã nguồn mở, phát hành theo giấy phép **GNU AGPL-3.0-or-later**. Xem [LICENSE](LICENSE), [LICENSE-EXCEPTIONS.md](LICENSE-EXCEPTIONS.md), [NOTICE.md](NOTICE.md) và [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

Nếu bạn phân phối PAM_Edit Video (kể cả cung cấp như một dịch vụ qua mạng), bạn phải cung cấp kèm mã nguồn theo đúng điều khoản AGPL.
