use x86_64::registers::control::{Cr4, Cr4Flags};

pub fn init_features() {
    unsafe {
        let mut cr4 = Cr4::read();
        
        // SMEP (Supervisor Mode Execution Prevention) を有効化
        cr4.insert(Cr4Flags::SUPERVISOR_MODE_EXECUTION_PROTECTION);
        
        // SMAP (Supervisor Mode Access Prevention) を有効化
        cr4.insert(Cr4Flags::SUPERVISOR_MODE_ACCESS_PREVENTION);

        // PKU/MPK (Memory Protection Keys) を有効化
        cr4.insert(Cr4Flags::PROTECTION_KEY_USER);
        // cr4.insert(Cr4Flags::PROTECTION_KEY_SUPERVISOR); // if supported

        Cr4::write(cr4);
    }
}
