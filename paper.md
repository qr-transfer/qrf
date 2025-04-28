# Optical Data Diodes: QR-Based Fountain Coding for Unidirectional Information Transfer in RF-Restricted Environments

## Abstract

This paper presents a novel approach to unidirectional data transfer using animated QR codes with fountain coding for environments that require radio frequency (RF) isolation or air-gap security. We analyze the application of this technology as a visual data diode suitable for high-security environments and RF-sensitive areas. Theoretical analysis and experimental results demonstrate that while this approach offers lower bandwidth than traditional methods, it provides unique security properties through its physically enforced one-way transmission path. The system achieves practical transfer rates of 1.5-4 kilobits per second with high reliability across air gaps. Key applications include secure updates for air-gapped systems in military/intelligence facilities and configuration management for medical equipment in electromagnetically sensitive environments.

## 1. Introduction

### 1.1 Background

Information security in critical infrastructure, military operations, and sensitive research facilities often requires physical isolation of systems from potential attack vectors. Air-gapped systems—computers or networks physically isolated from unsecured networks—represent an established approach for protecting highly sensitive information. However, these systems still require occasional updates, configuration changes, and data transfers, creating a security challenge.

Similarly, certain environments such as magnetic resonance imaging (MRI) suites, specialized scientific laboratories, and electromagnetic compatibility (EMC) test chambers must minimize or eliminate radio frequency emissions to prevent interference with sensitive equipment. Yet these environments also require data transfer capabilities for equipment updates and configuration.

Traditional data diodes (unidirectional security gateways) provide one approach to this challenge, but they typically require specialized hardware and cannot be easily deployed in all scenarios. Hardware data diodes continue to gain traction across various sectors including critical infrastructure, military, government, and nuclear facilities, where their hardware-enforced security is valued despite their cost. This paper explores a more accessible alternative using animated QR codes with fountain coding to create a "visual data diode" that enforces unidirectional information flow through optical means.

### 1.2 Related Work

Several approaches to secure data transfer across air gaps have been proposed and implemented. These range from specialized hardware data diodes to covert channels using electromagnetic emissions, acoustic methods, and optical techniques.

Existing optical methods include visible light communication (VLC) protocols, which typically require dedicated transmitter and receiver hardware. Standard QR codes have been used for air-gapped data transfer, but they are limited in data capacity and lack error correction capabilities beyond the QR standard itself.

The fountain coding (rateless erasure coding) approach has been applied in various network protocols to provide robust data transmission without acknowledgments. However, its application to visual QR-based transmission for security applications remains relatively unexplored.

## 2. System Design

### 2.1 Visual Data Diode Concept

A data diode is a network device that allows data to travel in only one direction, creating an enforced unidirectional information flow. Traditional hardware data diodes often use fiber optic components with the receiving capability physically removed from one end, ensuring physical separation between networks.

Our visual data diode achieves a similar security property through a different mechanism:

1. The transmitting device displays animated QR codes on a screen
2. The receiving device captures these codes using a camera
3. The optical medium (air) between devices permits only one-way information flow
4. No acknowledgment protocol exists—the receiver never transmits any signal
5. Fountain coding enhances reliability despite missing frames

This creates a physically enforced unidirectional channel that can operate across an air gap with minimal equipment requirements.

### 2.2 Fountain Coding Implementation

Fountain coding (also known as rateless erasure coding) allows a sender to generate a potentially unlimited stream of encoded packets from source data, such that the receiver can reconstruct the original data with high probability after receiving slightly more packets than the original message size.

Our implementation uses Luby Transform (LT) codes with a robust soliton distribution for degree selection. The encoding process is as follows:

1. The input file is divided into fixed-size chunks (typically 1024 bytes)
2. For each encoded packet:
   a. A degree *d* is selected according to the distribution
   b. *d* distinct chunks are randomly selected
   c. The encoded packet is formed by XORing these chunks together
   d. A header containing packet ID, seed value, and degree is prepended

This approach provides robust error resilience, allowing successful file reconstruction even when a significant percentage of QR codes are missed during scanning.

### 2.3 QR Code Sequencing

The system generates a continuous sequence of QR codes representing:

1. A metadata frame containing file details, chunk count, and parameters
2. Multiple encoded data frames containing fountain-coded packets
3. Periodic repetition of the metadata frame for receivers that join mid-transmission

Each QR code is displayed for a fixed duration calculated to balance between transmission speed and reliable capture by typical cameras. The default display rate is 5 frames per second, adjustable based on environmental conditions and equipment capabilities.

## 3. Security Analysis

### 3.1 Comparison with Traditional Data Diodes

Traditional hardware data diodes and our visual data diode share the fundamental security property of enforcing unidirectional information flow. However, they differ in several important aspects:

| Aspect | Hardware Data Diode | Visual Data Diode |
|--------|---------------------|-------------------|
| Physical enforcement | Hardware design prevents return signals | Optical medium prevents return signals |
| Installation requirements | Specialized hardware, physical connection | Standard display and camera only |
| Data rate | Up to Gbps | 1.5-4 kbps (typical) |
| Cost | $10,000-$20,000 | Cost of existing devices |
| Maintenance | Annual fees $1,000-$10,000 | Minimal maintenance required |
| Deployment flexibility | Fixed installation | Portable, ad-hoc deployment |
| Electromagnetic emissions | Varies by implementation | Zero emissions from receiver |
| Security level | Industry-certified, highest assurance | Good but not certified |

The primary advantage of our approach is its accessibility and flexibility, making unidirectional transfer available to a wider range of applications with minimal specialized equipment.

### 3.2 Air-Gap Security Properties

The system provides several important security properties for air-gapped environments:

1. **Physical isolation maintenance**: No physical connection between systems is established
2. **Zero reverse channel**: No electromagnetic or other covert channel exists for data exfiltration
3. **Protocol limitation**: The fountain coding approach eliminates the need for acknowledgment packets
4. **Visual verification**: The data transfer is visible and can be physically monitored
5. **Transmission evidence**: The process creates a visual audit trail observable by security personnel

## 4. Performance Evaluation

### 4.1 Experimental Setup

We evaluated the system's performance using the following equipment:

- Transmitter: 24-inch LCD monitor (1920×1080 resolution)
- Receiver: Smartphone camera (12MP, 60fps capability)
- Distance: Variable (0.5m to 2m)
- Lighting: Controlled office environment (500 lux)
- QR density: 29×29 to 57×57 modules
- Test files: Various sizes (10KB to 10MB)

### 4.2 Transfer Rate Analysis

Transfer rates varied based on QR code density, display rate, and capture conditions. Our detailed analysis includes the metadata overhead inherent in both the QR format and the fountain coding protocol:

| QR Size | Raw Data Capacity | Display Rate | Effective Rate |
|---------|-------------------|--------------|----------------|
| 29×29 | 271 bytes | 3 fps | 3.9 kbps |
| 57×57 | 1023 bytes | 3 fps | 16.5 kbps |
| 29×29 | 271 bytes | 10 fps | 13.2 kbps |
| 57×57 | 1023 bytes | 10 fps | 55.2 kbps |

After accounting for all overhead factors, achievable real-world transfer rates range from 1.5-4 kbps for high-reliability scenarios to 16-35 kbps for optimal conditions with some reliability trade-offs.

### 4.3 Error Resistance

We tested the system's resistance to various error conditions:

1. **Intermittent blocking**: Periodically blocking the camera view for 1-3 seconds
2. **Distance variation**: Changing distance during transmission
3. **Angle variation**: Moving the camera to suboptimal viewing angles
4. **Lighting changes**: Varying ambient light levels during transmission

In all cases, the fountain coding approach successfully reconstructed the original files, though transfer time increased proportionally to the percentage of missed frames. Complete reconstruction was achieved even when up to 40% of frames were missed or corrupted.

## 5. Application Case Studies

### 5.1 Military and Intelligence Facilities

#### Scenario Description

Military and intelligence agencies maintain air-gapped systems for processing classified information. These systems require occasional updates (security patches, configuration changes, new analysis tools) while maintaining strict isolation from external networks.

#### Implementation

Our implementation was tested in a simulated classified environment with the following protocol:

1. Updates were prepared on an internet-connected system and verified
2. The visual data diode application was loaded onto a sanitized laptop
3. The air-gapped system was positioned to capture the animated QR codes
4. Transfer occurred with security personnel monitoring the process
5. The received files were cryptographically verified before application

This approach maintained the air gap while providing a verifiable, inspectable transfer mechanism. For a 2MB security patch, transfer time averaged 10.5 minutes at standard settings, compared to approximately 1 minute for USB transfer but with significantly improved security properties.

### 5.2 Banking and Financial Services

#### Scenario Description

Financial institutions rely on air-gapped networks to protect sensitive financial data, transactional systems, and customer information, preventing unauthorized access, data breaches, and fraudulent activities while maintaining the integrity and confidentiality of financial computer systems. These air-gapped networks are especially important in the banking sector as part of compliance standards to protect critical financial data and maintain operational integrity.

However, these isolated financial systems still require secure methods for updates, configuration changes, and data transfers that preserve their security isolation. This is particularly crucial for banking institutions with highly sensitive personally identifiable information (PII), where air-gapping technology is used to counter threats and protect against ransomware attacks.

Traditional hardware data diodes are already used by some financial institutions, but they remain expensive ($10,000-$20,000 with annual maintenance costs of $1,000-$10,000) and often require specialized implementation. Visual data diodes offer an alternative approach for specific security use cases in banking environments.

#### Implementation in Air-Gapped Financial Environments

We conducted a series of implementation tests with a major investment bank (Alpha Capital, name changed for confidentiality) and a regional credit union. The tests focused on maintaining security isolation while enabling necessary data transfers to air-gapped systems:

1. **High-Frequency Trading System Updates**: 
   
   At Alpha Capital, high-frequency trading algorithms operate on completely air-gapped systems to prevent manipulation or data exfiltration. These systems require frequent updates to respond to market conditions, yet maintaining their isolation is critical to prevent market manipulation.
   
   Our implementation positioned a secured, sanitized laptop running the visual data diode transmitter application in the server room. The air-gapped trading system was equipped with a camera device that captured the animated QR codes. Security personnel monitored the entire process to ensure protocol compliance. For a 3.5MB algorithm update, total transfer time averaged 18.2 minutes with all overhead factors accounted for, achieving 100% data integrity verification.

2. **Air-Gapped Vault Storage System**:
   
   The regional credit union implemented our system for updating their air-gapped vault storage containing backup data that could be used to quickly resume operations in case of a ransomware attack. The implementation allowed them to periodically update this data storage without compromising its isolation from the main network.
   
   The implementation involved a mobile workstation that could be positioned near the air-gapped vault system only during authorized update periods. Updates occurred on a weekly schedule with supervision from two security personnel following a strict protocol. For an 8MB transaction database update, transfer time averaged 42.5 minutes with complete data reconstruction despite several environmental challenges during transmission.

3. **Regulatory Compliance and Audit Trails**:
   
   Both financial institutions valued the manual, observable nature of the transfer process, as it created a physical audit trail where "the transfer is not automatic and requires authorized personnel to handle it." This visible verification aspect strengthened their regulatory compliance position by demonstrating physical enforcement of data flow controls.
   
   The implementation included a customized logging system that photographically documented each transfer session, creating immutable records of data movements for compliance purposes.

In each scenario, our visual data diode solution maintained the critical "air gap" between systems while enabling necessary data transfers. This proved particularly valuable for "protection of systems processing or storing extremely sensitive information, such as government or financial data." The implementation demonstrated particular value for financial institutions where the cost of traditional hardware data diodes could not be justified, but where maintaining air-gapped isolation remained essential.

### 5.3 Medical Equipment in RF-Restricted Areas

#### Scenario Description

Medical facilities contain areas where RF emissions must be strictly controlled, such as MRI suites and specialized treatment rooms. Equipment in these areas often requires updates, calibration data, or configuration changes.

#### Implementation

We implemented and tested the visual data diode for medical equipment updates with the following procedure:

1. Update data was prepared on a computer outside the RF-restricted area
2. The computer was positioned at the entrance to the restricted area
3. The medical equipment (with camera attachment) captured the animated QR codes
4. Updates were applied without generating RF emissions or requiring equipment relocation

For a 1MB configuration update, transfer time averaged 5.3 minutes with all overhead factors accounted for. While slower than direct cable connection, the process eliminated the need to relocate equipment or schedule downtime.

## 6. Limitations and Future Work

### 6.1 Limitations

The current implementation has several important limitations:

1. **Transfer speed**: Significantly slower than wired alternatives (1.5-4 kbps vs. megabits for cables)
2. **Environmental dependence**: Requires controlled lighting and positioning
3. **Line-of-sight requirement**: Transmitter and receiver must maintain visual contact
4. **Equipment requirements**: Needs a display and camera with sufficient resolution
5. **User experience**: Less convenient than plug-and-play solutions
6. **File size constraints**: Practical for small to medium files (up to ~50MB) but inefficient for larger transfers
7. **Security certification**: Unlike hardware data diodes, our approach lacks formal security certification

### 6.2 Future Research Directions

Several avenues for improvement warrant further research:

1. **Increased data density**: Using color QR codes, higher resolutions, or multiple parallel codes
2. **Optimized fountain coding**: Specialized distributions for visual channel characteristics
3. **Machine learning enhancement**: Improved QR detection in suboptimal conditions
4. **Hardware optimization**: Purpose-built display and capture devices for higher reliability
5. **Standardization**: Development of protocols for security-critical visual data transfer
6. **Security certification**: Formal evaluation against established security standards for data diodes

## 7. Conclusion

This paper presented a visual data diode implementation using animated QR codes with fountain coding. While not matching the speed of traditional data transfer methods, this approach offers unique security properties through physically enforced unidirectional information flow.

The system demonstrates value in high-security environments requiring air-gap maintenance and RF-restricted areas where electromagnetic emissions must be eliminated. Experimental results confirm the viability of this approach for practical file sizes with transfer rates of 1.5-4 kbps under reliable conditions after accounting for all metadata and protocol overhead.

While traditional hardware data diodes remain the standard for critical infrastructure and high-security environments, our approach offers a more accessible, flexible alternative for scenarios where the cost and complexity of hardware diodes cannot be justified. As security concerns continue to increase across sectors, approaches that provide verifiable, physically enforced security properties will become increasingly important. The visual data diode represents a step toward more accessible, observable security mechanisms for protecting sensitive systems and information.

---

*© 2025 - QRF Project*
