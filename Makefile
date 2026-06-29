KERNEL_NAME := pangea-kernel
TARGET      := x86_64-unknown-none
ISO_NAME    := pangea-os.iso
ISO_DIR     := iso_root

.PHONY: all build iso run clean

all: run

build:
	@echo "[+] Compiling PangeaOS Kernel..."
	@env -u RUSTFLAGS cargo build -p $(KERNEL_NAME)

iso: build
	@echo "[+] Crafting Bootable ISO Image..."
	@rm -f $(ISO_NAME)
	@cp target/$(TARGET)/debug/$(KERNEL_NAME) $(ISO_DIR)/
	@xorriso -as mkisofs -b limine-bios-cd.bin \
		-no-emul-boot -boot-load-size 4 -boot-info-table \
		--efi-boot limine-uefi-cd.bin \
		-efi-boot-part --efi-boot-image --protective-msdos-label \
		$(ISO_DIR) -o $(ISO_NAME) > /dev/null 2>&1
	@echo "[+] Injecting Limine MBR Bootloader..."
	@nix run nixpkgs#limine -- bios-install $(ISO_NAME) > /dev/null 2>&1

run: iso
	@echo "[+] Booting PangeaOS in QEMU..."
	@qemu-system-x86_64 -cpu max -smp 4 -m 2G -cdrom $(ISO_NAME) -serial stdio -netdev user,id=n0,hostfwd=tcp::8888-:80 -device e1000,netdev=n0

clean:
	@echo "[+] Purging build artifacts..."
	@cargo clean
	@rm -f $(ISO_NAME)
