
# Defense Network Security / FPGA Packet Filtering Marketplace

The defense network security / FPGA packet filtering space has several layers of competition, and where pacgate + pacinet would fit depends on which segment you're targeting. Here's the landscape:

## Direct Competitors: FPGA-Based Hardware Filtering

### Owl Cyber Defense (Columbia, MD) 

The closest direct competitor and the 800-pound gorilla in this space. They're a pure-play cybersecurity company focused on made-in-the-USA data diode and cross domain solutions, trusted to protect the most sensitive government and commercial networks worldwide.
Owl Cyber Defense's approach is strikingly similar to pacgate's philosophy — they use FPGA-based hardware filtering with modular, pre-tested functional blocks and a toolchain that configures mature functions into custom deployment images, rather than writing custom HDL per deployment.
Owl Cyber Defense's own product line spans from the Owl Talon data diode platform offering hardware-enforced one-way transfers up to 100Gbps
to NSA-approved cross domain solutions on the NCDSMO Baseline for streaming voice, video, and structured data.

### Carahsoft

They're ITAR-controlled and their CDS products are SABI and TSABI certified for classified networks.

### Owl Cyber Defense

** The key difference: Owl is opaque, proprietary, and expensive. Their toolchain is internal. Pacgate's compiler-based approach — YAML rules in, formally verified Verilog out — is fundamentally more transparent and auditable, which is increasingly what DoD acquisition programs want. **

### Everfox (Herndon, VA, formerly Forcepoint Federal) 

The other major CDS vendor. They recently acquired Garrison Technology specifically for Garrison's FPGA security technology to deliver nation-state level security for governments and regulated industries.
Their cross domain hardware portfolio includes FPGA and optical data separation to securely connect classified networks to high-threat networks.
Everfox came out of the Raytheon/Websense lineage — Raytheon acquired an 80% stake in Websense for $1.9 billion in 2015, and TPG later acquired the government cyber business for $2.45 billion in 2023.
So this is a well-funded operation.

### Garrison Technology (now part of Everfox) 

Worth calling out separately because their "hardsec" approach — using FPGAs to enforce security at the hardware level rather than trusting software — is philosophically identical to what pacgate does. Their innovation was specifically applying FPGA pixel-pushing to cross-domain browsing isolation, but the underlying thesis is the same: hardware-enforced security with a reduced attack surface because FPGAs have finite, provable states.

## Adjacent Competitors: SmartNICs and P4-Programmable Hardware

### AMD/Xilinx Alveo SmartNICs 

The SN1000 directly offloads CPU-intensive tasks with an architecture that accelerates custom offloads at line rate, powered by an XCU26 FPGA and a 16-core ARM processor.

### Xilinx

These support P4 programmability for custom packet processing pipelines. This is the commercial data center version of what pacgate does, but aimed at cloud providers rather than defense. Not a direct competitor, but it defines the performance bar.

### nCk Research + BittWare 

They demonstrated a 100G network packet broker with PCAP filtering at full line rate on BittWare's UltraScale FPGA boards

Electronics Specifier
, where users express filters in standard PCAP syntax and the toolchain optimizes and loads them into the FPGA. This is actually the closest commercial analog to pacgate's compiler approach — high-level rule language in, FPGA implementation out — but they target commercial packet brokers, not defense filtering.

Napatech — Danish company making FPGA-based SmartNICs for telecom, cybersecurity, and financial applications. Their NT200A02 supports 100G and handles tunnel protocols (GTP, VXLAN, NVGRE) similar to pacgate's protocol support. They sell to defense indirectly through system integrators.
SDN Layer Competitors (Where PaciNet Would Compete)

Juniper Networks is deeply embedded in defense SDN. Lockheed Martin and Juniper demonstrated mission-aware routing technology for DoD that prioritizes data flow so commanders receive critical information first, delivered over available data links in a heterogeneous network-of-networks.
Lockheed Martin
Their Session Smart Routing + Mist AI platform is FedRAMP-authorized and extensively deployed across federal agencies and armed forces.
Juniper Networks
However, Juniper's SDN is about routing and WAN optimization — not hardware-enforced packet filtering. They're control plane, not data plane.

Cisco — Cisco demonstrated its SD-WAN solution integrated with Cisco Secure Firewalls at USAISEC, showing how it enhances network resilience by providing redundant and diverse network paths.
www.army.mil
Dominant market position but all software-based filtering running on x86, with the inherent vulnerabilities that entails.

Lockheed Martin DDLM — Their Dynamic Data Link Manager is the military-specific SDN controller that sits above Juniper/Cisco routing. This is actually closest to what pacinet would be, but it's a massive integrated platform, not a focused packet filtering orchestrator.
Rugged FPGA Board Vendors (Hardware Supply Chain)

### Curtiss-Wright Defense Solutions 

Partners with AMD for Versal Premium Adaptive SoC and FPGA processing cards using open standards like SOSA, CMOSS, and OpenVPX.
They make the ruggedized 3U/6U VPX boards that go into tactical systems. Not a competitor — potentially a deployment platform for pacgate.

### Mercury Systems (now part of Andover) 

Similar to Curtiss-Wright, ruggedized FPGA processing modules. Interesting that Owl's CEO came from Mercury.

### WOLF Advanced Technology, Kontron, Pentek 

All make defense-grade FPGA boards for signal processing and network processing in VPX form factors.
Where PacGate + PaciNet Has a Genuine Opening

The gap in the market is this: Owl and Everfox sell black boxes. You buy their appliance, it does what it does, and you can't audit the filter logic at the RTL level. The big SDN vendors (Juniper, Cisco) are all software-based filtering, which means the entire TCP/IP stack is an attack surface. P4 and SmartNIC approaches (Alveo, Napatech) are powerful but not designed for formal verification of filter correctness.

Pacgate's unique value proposition for defense is the combination of:

1.  Compiler-generated RTL from human-readable YAML — auditable by security reviewers who don't know Verilog
1.  Formal verification via SVA assertions proving the filter does exactly what the spec says
1.  Open toolchain — the customer owns the entire pipeline from rules to bitstream
1.  Wire-speed filtering without a software stack to exploit
1.  Reproducible builds — same YAML always produces the same Verilog, enabling deterministic security audits

With pacinet layered on top, you'd add centralized policy management and telemetry across multiple pacgate nodes — but with the same transparency and formal verification guarantees that the black-box vendors can't offer.

The NCDSMO certification path would be the long game, but even without it, there's an immediate market for pacgate as a boundary protection device at the enclave level, where NIST SP800-171 requires defense-in-depth but doesn't mandate a specific NCDSMO-listed product.
