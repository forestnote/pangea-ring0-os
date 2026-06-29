const bootSequence = [
    { text: "=====================================", class: "log-line" },
    { text: "PangeaOS Ring 0: The Singularity Engine", class: "log-line" },
    { text: "=====================================", class: "log-line" },
    { text: "[ OK ] Ring 0 Exclusive GDT & TSS Loaded. IST Active.", class: "log-info" },
    { text: "[ OK ] True IDT Loaded. Exception Barrier Active.", class: "log-info" },
    { text: "[ OK ] 8259 PIC Disabled (All Masked).", class: "log-warn" },
    { text: "[ INFO ] APIC Physical Base: 0xFEE00000", class: "log-line" },
    { text: "[ OK ] Local APIC Initialized. Timer Active.", class: "log-info" },
    { text: "[ ASH ] Booting Ring 0 Sandbox VM (FFI & Callback Mode)...", class: "log-ash" },
    { text: "[ ASH JIT ] Compiling Bytecode to Native x86_64...", class: "log-ash" },
    { text: "[ ASH JIT ] Sealing Memory Page (W^X Enforcer Active)...", class: "log-warn" },
    { text: "[ ASH JIT ] Emission Complete. Direct Execution Initiated...", class: "log-info" },
    { text: "[ ASH JIT LOG ] Value: 0xc0a80101 (3232235777)", class: "log-line" },
    { text: "        -> [ ASH JIT ] Native Result: 1", class: "log-info" },
    { text: "        -> [ ASH JIT ] Loop Calculation Sum (State[1]): 5050", class: "log-line" },
    { text: "        -> [ ASH JIT ] JIT Execution Time (TSC Ticks): 4420388", class: "log-info" },
    { text: "root@pangea-ring0:~# _", class: "log-line blink" }
];

const termBody = document.getElementById('term-body');
let currentLine = 0;

function typeLine() {
    if (currentLine < bootSequence.length) {
        const lineData = bootSequence[currentLine];
        const lineEl = document.createElement('div');
        lineEl.className = `log-line ${lineData.class}`;
        termBody.appendChild(lineEl);
        
        let charIndex = 0;
        const typingInterval = setInterval(() => {
            if (charIndex < lineData.text.length) {
                lineEl.textContent += lineData.text.charAt(charIndex);
                charIndex++;
            } else {
                clearInterval(typingInterval);
                currentLine++;
                termBody.scrollTop = termBody.scrollHeight;
                setTimeout(typeLine, Math.random() * 200 + 50);
            }
        }, 10);
    }
}

const observer = new IntersectionObserver((entries) => {
    entries.forEach(entry => {
        if (entry.isIntersecting) {
            setTimeout(typeLine, 500);
            observer.disconnect();
        }
    });
}, { threshold: 0.5 });

observer.observe(document.querySelector('.terminal-section'));
