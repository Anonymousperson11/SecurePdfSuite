# 01 — 核心加密、`.spdf` 容器格式与防篡改配额

本模块位于 `SecurePdf.Core`，负责 `.spdf` 的密码学封装、设备网络指纹采集以及双备份本地打印/有效期状态存储。

> [!NOTE]
> 所有加密操作均在纯内存中完成；`LocalQuotaStore` 同时读写 LocalAppData DPAPI 文件与 HKCU 注册表，用于检测单边删改与系统时间回拨。

## 目录

1. [`Directory.Build.props`](#directorybuildprops) — 全局 MSBuild 构建属性（C# 12、x64 目标平台、确定性编译与版本号）

2. [`src/SecurePdf.Core/SecurePdf.Core.csproj`](#srcsecurepdfcoresecurepdfcorecsproj) — 核心库项目文件，依赖 `BouncyCastle.Cryptography` 与 Windows 注册表/安全包

3. [`src/SecurePdf.Core/Crypto/CodeAndCryptoService.cs`](#srcsecurepdfcorecryptocodeandcryptoservicecs) — 随机验证码生成（剔除易混淆字符）、scrypt 密钥派生、AES-256-GCM 加解密与内存擦除

4. [`src/SecurePdf.Core/Format/SpdfContainer.cs`](#srcsecurepdfcoreformatspdfcontainercs) — `.spdf` v2 二进制文件头定义、策略序列化（含 `ExpiresAtUtc` 与摩尔纹/水印标志位）与 AAD 完整性绑定

5. [`src/SecurePdf.Core/Licensing/DeviceAndNetworkContext.cs`](#srcsecurepdfcorelicensingdeviceandnetworkcontextcs) — 采集机器 GUID、BIOS UUID、系统卷序列号生成机器指纹码，并获取当前活跃主网卡 IPv4 地址

6. [`src/SecurePdf.Core/Storage/LocalQuotaStore.cs`](#srcsecurepdfcorestoragelocalquotastorecs) — 打印次数与过期时间的 DPAPI + 注册表双存储状态机，含 5 分钟容差的系统时间回拨锁定


---

## `Directory.Build.props`


**模块说明**：全局 MSBuild 构建属性（C# 12、x64 目标平台、确定性编译与版本号）（共 `12` 行）


```xml
<Project>
  <PropertyGroup>
    <LangVersion>12.0</LangVersion>
    <Nullable>enable</Nullable>
    <ImplicitUsings>false</ImplicitUsings>
    <PlatformTarget>x64</PlatformTarget>
    <Deterministic>true</Deterministic>
    <Version>1.0.1</Version>
    <Company>SecurePdfSuite</Company>
    <Product>SecurePdfSuite</Product>
  </PropertyGroup>
</Project>
```


## `src/SecurePdf.Core/SecurePdf.Core.csproj`


**模块说明**：核心库项目文件，依赖 `BouncyCastle.Cryptography` 与 Windows 注册表/安全包（共 `19` 行）


```xml
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net48</TargetFramework>
    <AllowUnsafeBlocks>true</AllowUnsafeBlocks>
    <UseWindowsForms>false</UseWindowsForms>
  </PropertyGroup>

  <ItemGroup>
    <PackageReference Include="Microsoft.NETFramework.ReferenceAssemblies.net48" Version="1.0.3" PrivateAssets="All" />
    <PackageReference Include="BouncyCastle.Cryptography" Version="2.4.0" />
    <PackageReference Include="PdfiumViewer.Native.x86_64.v8-xfa" Version="2018.4.8.256" />
  </ItemGroup>

  <ItemGroup>
    <Reference Include="System.Drawing" />
    <Reference Include="System.Management" />
    <Reference Include="System.Security" />
  </ItemGroup>
</Project>
```


## `src/SecurePdf.Core/Crypto/CodeAndCryptoService.cs`


**模块说明**：随机验证码生成（剔除易混淆字符）、scrypt 密钥派生、AES-256-GCM 加解密与内存擦除（共 `178` 行）


```csharp
using System;
using System.Runtime.CompilerServices;
using System.Security.Cryptography;
using System.Text;
using Org.BouncyCastle.Crypto;
using Org.BouncyCastle.Crypto.Digests;
using Org.BouncyCastle.Crypto.Engines;
using Org.BouncyCastle.Crypto.Generators;
using Org.BouncyCastle.Crypto.Macs;
using Org.BouncyCastle.Crypto.Modes;
using Org.BouncyCastle.Crypto.Parameters;

namespace SecurePdf.Core.Crypto
{
    public static class CodeAndCryptoService
    {
        private const string CodeAlphabet = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

        public static string GenerateVerificationCode(int groupCount = 3, int groupLength = 4)
        {
            if (groupCount <= 0 || groupLength <= 0)
            {
                throw new ArgumentOutOfRangeException(nameof(groupCount));
            }

            int totalChars = groupCount * groupLength;
            byte[] randomBytes = GenerateRandomBytes(totalChars * 2);
            var sb = new StringBuilder(totalChars + groupCount - 1);

            int byteIdx = 0;
            for (int i = 0; i < totalChars; i++)
            {
                if (i > 0 && i % groupLength == 0)
                {
                    sb.Append('-');
                }
                byte val = randomBytes[byteIdx++];
                sb.Append(CodeAlphabet[val & 0x1F]);
            }

            ZeroMemory(randomBytes);
            return sb.ToString();
        }

        public static string NormalizeCode(string code)
        {
            if (string.IsNullOrWhiteSpace(code))
            {
                return string.Empty;
            }

            var sb = new StringBuilder(code.Length);
            foreach (char ch in code)
            {
                if (char.IsWhiteSpace(ch) || ch == '-')
                {
                    continue;
                }
                sb.Append(char.ToUpperInvariant(ch));
            }
            return sb.ToString();
        }

        public static byte[] GenerateRandomBytes(int length)
        {
            byte[] data = new byte[length];
            using (var rng = new RNGCryptoServiceProvider())
            {
                rng.GetBytes(data);
            }
            return data;
        }

        public static byte[] DeriveKeyScrypt(
            string normalizedCode,
            byte[] salt,
            int costN = 16384,
            int blockSizeR = 8,
            int parallelP = 1,
            int outputLength = 32)
        {
            if (string.IsNullOrEmpty(normalizedCode))
            {
                throw new ArgumentException("Verification code cannot be empty.", nameof(normalizedCode));
            }

            byte[] passwordBytes = Encoding.UTF8.GetBytes(normalizedCode);
            try
            {
                return SCrypt.Generate(passwordBytes, salt, costN, blockSizeR, parallelP, outputLength);
            }
            finally
            {
                ZeroMemory(passwordBytes);
            }
        }

        public static byte[] EncryptAesGcm(byte[] key, byte[] nonce, byte[] plaintext, byte[]? aad = null)
        {
            var cipher = new GcmBlockCipher(new AesEngine());
            var parameters = new AeadParameters(new KeyParameter(key), 128, nonce, aad);
            cipher.Init(true, parameters);

            byte[] output = new byte[cipher.GetOutputSize(plaintext.Length)];
            int len = cipher.ProcessBytes(plaintext, 0, plaintext.Length, output, 0);
            cipher.DoFinal(output, len);
            return output;
        }

        public static byte[] DecryptAesGcm(byte[] key, byte[] nonce, byte[] ciphertextWithTag, byte[]? aad = null)
        {
            var cipher = new GcmBlockCipher(new AesEngine());
            var parameters = new AeadParameters(new KeyParameter(key), 128, nonce, aad);
            cipher.Init(false, parameters);

            byte[] output = new byte[cipher.GetOutputSize(ciphertextWithTag.Length)];
            int len = cipher.ProcessBytes(ciphertextWithTag, 0, ciphertextWithTag.Length, output, 0);
            try
            {
                cipher.DoFinal(output, len);
                return output;
            }
            catch (InvalidCipherTextException ex)
            {
                ZeroMemory(output);
                throw new CryptographicException("Authentication failed: invalid verification code or tampered data.", ex);
            }
        }

        public static byte[] ComputeSha256(byte[] data)
        {
            using (var sha = SHA256.Create())
            {
                return sha.ComputeHash(data);
            }
        }

        public static byte[] ComputeHmacSha256(byte[] key, byte[] data)
        {
            var hmac = new HMac(new Sha256Digest());
            hmac.Init(new KeyParameter(key));
            hmac.BlockUpdate(data, 0, data.Length);
            byte[] result = new byte[hmac.GetMacSize()];
            hmac.DoFinal(result, 0);
            return result;
        }

        [MethodImpl(MethodImplOptions.NoInlining | MethodImplOptions.NoOptimization)]
        public static bool FixedTimeEquals(byte[] a, byte[] b)
        {
            if (a == null || b == null || a.Length != b.Length)
            {
                return false;
            }

            int diff = 0;
            for (int i = 0; i < a.Length; i++)
            {
                diff |= a[i] ^ b[i];
            }
            return diff == 0;
        }

        [MethodImpl(MethodImplOptions.NoInlining | MethodImplOptions.NoOptimization)]
        public static void ZeroMemory(byte[]? buffer)
        {
            if (buffer == null)
            {
                return;
            }

            for (int i = 0; i < buffer.Length; i++)
            {
                buffer[i] = 0;
            }
        }
    }
}
```


## `src/SecurePdf.Core/Format/SpdfContainer.cs`


**模块说明**：`.spdf` v2 二进制文件头定义、策略序列化（含 `ExpiresAtUtc` 与摩尔纹/水印标志位）与 AAD 完整性绑定（共 `380` 行）


```csharp
using System;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using SecurePdf.Core.Crypto;

namespace SecurePdf.Core.Format
{
    public enum MoireStrength : byte
    {
        Off = 0,
        Low = 1,
        Medium = 2,
        High = 3
    }

    [Flags]
    public enum SpdfSecurityFlags : uint
    {
        None = 0,
        EnableHiddenWatermark = 1 << 0,
        EnableBlurOnUnfocus = 1 << 1,
        AllowUserAdjustMoire = 1 << 2,
        EnableVisibleWatermark = 1 << 3
    }

    public sealed class SpdfCreationOptions
    {
        public int MaxPrints { get; set; } = 1;
        public DateTimeOffset? ExpiresAtUtc { get; set; }
        public MoireStrength MoireLevel { get; set; } = MoireStrength.Medium;
        public SpdfSecurityFlags SecurityFlags { get; set; } =
            SpdfSecurityFlags.EnableHiddenWatermark |
            SpdfSecurityFlags.EnableBlurOnUnfocus;
        public string OriginalFileName { get; set; } = "document.pdf";
        public string WatermarkNote { get; set; } = string.Empty;
        public string? CustomVerificationCode { get; set; }

        public int KdfCostN { get; set; } = 16384;
        public int KdfBlockSizeR { get; set; } = 8;
        public int KdfParallelP { get; set; } = 1;
    }

    public sealed class SpdfPolicy
    {
        public Guid DocId { get; set; }
        public int MaxPrints { get; set; }
        public DateTimeOffset? ExpiresAtUtc { get; set; }
        public MoireStrength MoireLevel { get; set; }
        public SpdfSecurityFlags SecurityFlags { get; set; }
        public string OriginalFileName { get; set; } = string.Empty;
        public string WatermarkNote { get; set; } = string.Empty;
    }

    public sealed class SpdfEnvelopeInfo
    {
        public Guid DocId { get; set; }
        public ushort FormatVersion { get; set; }
        public long FileSizeBytes { get; set; }
    }

    public sealed class UnlockedSpdfDocument : IDisposable
    {
        private byte[]? _pdfBytes;
        private bool _disposed;

        public UnlockedSpdfDocument(SpdfPolicy policy, byte[] pdfBytes)
        {
            Policy = policy ?? throw new ArgumentNullException(nameof(policy));
            _pdfBytes = pdfBytes ?? throw new ArgumentNullException(nameof(pdfBytes));
        }

        public SpdfPolicy Policy { get; }

        public byte[] PdfBytes
        {
            get
            {
                if (_disposed || _pdfBytes == null)
                {
                    throw new ObjectDisposedException(nameof(UnlockedSpdfDocument));
                }
                return _pdfBytes;
            }
        }

        public void Dispose()
        {
            if (_disposed)
            {
                return;
            }
            _disposed = true;
            CodeAndCryptoService.ZeroMemory(_pdfBytes);
            _pdfBytes = null;
        }
    }

    public static class SpdfContainer
    {
        private static readonly byte[] MagicBytes = Encoding.ASCII.GetBytes("SPDF");
        private const ushort CurrentVersion = 2;
        private const int SaltSize = 16;
        private const int NonceSize = 12;
        private const int DocKeySize = 32;

        public static (Guid DocId, string VerificationCode) Create(
            byte[] rawPdfBytes,
            string outputSpdfPath,
            SpdfCreationOptions options)
        {
            if (rawPdfBytes == null || rawPdfBytes.Length < 8)
            {
                throw new ArgumentException("Input PDF bytes are invalid or empty.", nameof(rawPdfBytes));
            }
            if (options == null)
            {
                throw new ArgumentNullException(nameof(options));
            }
            if (options.MaxPrints < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(options.MaxPrints), "MaxPrints cannot be negative.");
            }
            if (!(rawPdfBytes[0] == '%' && rawPdfBytes[1] == 'P' && rawPdfBytes[2] == 'D' && rawPdfBytes[3] == 'F'))
            {
                throw new InvalidDataException("Source file does not appear to be a valid PDF (%PDF header missing).");
            }

            string verificationCode = string.IsNullOrWhiteSpace(options.CustomVerificationCode)
                ? CodeAndCryptoService.GenerateVerificationCode()
                : options.CustomVerificationCode!.Trim().ToUpperInvariant();

            string normalizedCode = CodeAndCryptoService.NormalizeCode(verificationCode);
            if (normalizedCode.Length < 4)
            {
                throw new ArgumentException("Verification code must contain at least 4 alphanumeric characters.");
            }

            Guid docId = Guid.NewGuid();
            byte[] fileSalt = CodeAndCryptoService.GenerateRandomBytes(SaltSize);
            byte[] docKey = CodeAndCryptoService.GenerateRandomBytes(DocKeySize);
            byte[] keyWrapNonce = CodeAndCryptoService.GenerateRandomBytes(NonceSize);
            byte[] payloadNonce = CodeAndCryptoService.GenerateRandomBytes(NonceSize);
            byte[]? kek = null;
            byte[]? policyPlaintext = null;

            try
            {
                kek = CodeAndCryptoService.DeriveKeyScrypt(
                    normalizedCode,
                    fileSalt,
                    options.KdfCostN,
                    options.KdfBlockSizeR,
                    options.KdfParallelP);

                policyPlaintext = SerializePolicyAndKey(docKey, options);
                byte[] aad = BuildEnvelopeAad(docId, CurrentVersion, fileSalt, options.KdfCostN, options.KdfBlockSizeR, options.KdfParallelP);

                byte[] wrappedPolicy = CodeAndCryptoService.EncryptAesGcm(kek, keyWrapNonce, policyPlaintext, aad);
                byte[] encryptedPayload = CodeAndCryptoService.EncryptAesGcm(docKey, payloadNonce, rawPdfBytes, aad);

                string? dir = Path.GetDirectoryName(outputSpdfPath);
                if (!string.IsNullOrEmpty(dir))
                {
                    Directory.CreateDirectory(dir);
                }

                using (var fs = new FileStream(outputSpdfPath, FileMode.Create, FileAccess.Write, FileShare.None))
                using (var writer = new BinaryWriter(fs, Encoding.UTF8, leaveOpen: false))
                {
                    writer.Write(MagicBytes);
                    writer.Write(CurrentVersion);
                    writer.Write(docId.ToByteArray());
                    writer.Write(fileSalt);
                    writer.Write(options.KdfCostN);
                    writer.Write(options.KdfBlockSizeR);
                    writer.Write(options.KdfParallelP);
                    writer.Write(keyWrapNonce);
                    writer.Write(wrappedPolicy.Length);
                    writer.Write(wrappedPolicy);
                    writer.Write(payloadNonce);
                    writer.Write(encryptedPayload.LongLength);
                    writer.Write(encryptedPayload);
                }

                return (docId, verificationCode);
            }
            finally
            {
                CodeAndCryptoService.ZeroMemory(docKey);
                CodeAndCryptoService.ZeroMemory(kek);
                CodeAndCryptoService.ZeroMemory(policyPlaintext);
            }
        }

        public static SpdfEnvelopeInfo ReadEnvelope(string spdfPath)
        {
            using (var fs = new FileStream(spdfPath, FileMode.Open, FileAccess.Read, FileShare.Read))
            using (var reader = new BinaryReader(fs, Encoding.UTF8, leaveOpen: false))
            {
                byte[] magic = reader.ReadBytes(4);
                if (magic.Length != 4 ||
                    magic[0] != MagicBytes[0] || magic[1] != MagicBytes[1] ||
                    magic[2] != MagicBytes[2] || magic[3] != MagicBytes[3])
                {
                    throw new InvalidDataException("Not a valid .spdf file.");
                }

                ushort version = reader.ReadUInt16();
                if (version != CurrentVersion)
                {
                    throw new NotSupportedException($"Unsupported .spdf version: {version}");
                }

                Guid docId = new Guid(reader.ReadBytes(16));
                return new SpdfEnvelopeInfo
                {
                    DocId = docId,
                    FormatVersion = version,
                    FileSizeBytes = fs.Length
                };
            }
        }

        public static UnlockedSpdfDocument Unlock(string spdfPath, string verificationCode)
        {
            string normalizedCode = CodeAndCryptoService.NormalizeCode(verificationCode);
            if (string.IsNullOrEmpty(normalizedCode))
            {
                throw new CryptographicException("Verification code is required.");
            }

            using (var fs = new FileStream(spdfPath, FileMode.Open, FileAccess.Read, FileShare.Read))
            using (var reader = new BinaryReader(fs, Encoding.UTF8, leaveOpen: false))
            {
                byte[] magic = reader.ReadBytes(4);
                if (magic.Length != 4 ||
                    magic[0] != MagicBytes[0] || magic[1] != MagicBytes[1] ||
                    magic[2] != MagicBytes[2] || magic[3] != MagicBytes[3])
                {
                    throw new InvalidDataException("Not a valid .spdf file.");
                }

                ushort version = reader.ReadUInt16();
                if (version != CurrentVersion)
                {
                    throw new NotSupportedException($"Unsupported .spdf version: {version}");
                }

                Guid docId = new Guid(reader.ReadBytes(16));
                byte[] fileSalt = reader.ReadBytes(SaltSize);
                int kdfCostN = reader.ReadInt32();
                int kdfBlockSizeR = reader.ReadInt32();
                int kdfParallelP = reader.ReadInt32();

                if (kdfCostN < 1024 || kdfCostN > 1048576 || (kdfCostN & (kdfCostN - 1)) != 0 ||
                    kdfBlockSizeR <= 0 || kdfBlockSizeR > 64 || kdfParallelP <= 0 || kdfParallelP > 16)
                {
                    throw new InvalidDataException("Corrupted KDF parameters in .spdf header.");
                }

                byte[] keyWrapNonce = reader.ReadBytes(NonceSize);
                int wrappedLen = reader.ReadInt32();
                if (wrappedLen <= 16 || wrappedLen > 1024 * 1024)
                {
                    throw new InvalidDataException("Invalid policy block length.");
                }
                byte[] wrappedPolicy = reader.ReadBytes(wrappedLen);

                byte[] payloadNonce = reader.ReadBytes(NonceSize);
                long payloadLen = reader.ReadInt64();
                if (payloadLen <= 16 || payloadLen > fs.Length - fs.Position)
                {
                    throw new InvalidDataException("Invalid encrypted payload length.");
                }
                byte[] encryptedPayload = reader.ReadBytes(checked((int)payloadLen));

                byte[] aad = BuildEnvelopeAad(docId, version, fileSalt, kdfCostN, kdfBlockSizeR, kdfParallelP);
                byte[]? kek = null;
                byte[]? policyPlaintext = null;
                byte[]? docKey = null;

                try
                {
                    kek = CodeAndCryptoService.DeriveKeyScrypt(
                        normalizedCode,
                        fileSalt,
                        kdfCostN,
                        kdfBlockSizeR,
                        kdfParallelP);

                    policyPlaintext = CodeAndCryptoService.DecryptAesGcm(kek, keyWrapNonce, wrappedPolicy, aad);
                    (docKey, SpdfPolicy policy) = DeserializePolicyAndKey(docId, policyPlaintext);
                    byte[] pdfBytes = CodeAndCryptoService.DecryptAesGcm(docKey, payloadNonce, encryptedPayload, aad);

                    return new UnlockedSpdfDocument(policy, pdfBytes);
                }
                finally
                {
                    CodeAndCryptoService.ZeroMemory(kek);
                    CodeAndCryptoService.ZeroMemory(policyPlaintext);
                    CodeAndCryptoService.ZeroMemory(docKey);
                }
            }
        }

        private static byte[] BuildEnvelopeAad(
            Guid docId,
            ushort version,
            byte[] salt,
            int costN,
            int blockSizeR,
            int parallelP)
        {
            using (var ms = new MemoryStream(64))
            using (var w = new BinaryWriter(ms))
            {
                w.Write(MagicBytes);
                w.Write(version);
                w.Write(docId.ToByteArray());
                w.Write(salt);
                w.Write(costN);
                w.Write(blockSizeR);
                w.Write(parallelP);
                return ms.ToArray();
            }
        }

        private static byte[] SerializePolicyAndKey(byte[] docKey, SpdfCreationOptions options)
        {
            using (var ms = new MemoryStream())
            using (var w = new BinaryWriter(ms, Encoding.UTF8))
            {
                w.Write((byte)1);
                w.Write(docKey);
                w.Write(options.MaxPrints);
                w.Write(options.ExpiresAtUtc.HasValue ? options.ExpiresAtUtc.Value.ToUniversalTime().Ticks : 0L);
                w.Write((byte)options.MoireLevel);
                w.Write((uint)options.SecurityFlags);
                w.Write(options.OriginalFileName ?? string.Empty);
                w.Write(options.WatermarkNote ?? string.Empty);
                return ms.ToArray();
            }
        }

        private static (byte[] DocKey, SpdfPolicy Policy) DeserializePolicyAndKey(Guid docId, byte[] plaintext)
        {
            using (var ms = new MemoryStream(plaintext))
            using (var r = new BinaryReader(ms, Encoding.UTF8))
            {
                byte innerVersion = r.ReadByte();
                if (innerVersion != 1)
                {
                    throw new NotSupportedException($"Unsupported inner policy version: {innerVersion}");
                }

                byte[] docKey = r.ReadBytes(DocKeySize);
                int maxPrints = r.ReadInt32();
                long expiresTicks = r.ReadInt64();
                MoireStrength moire = (MoireStrength)r.ReadByte();
                SpdfSecurityFlags flags = (SpdfSecurityFlags)r.ReadUInt32();
                string fileName = r.ReadString();
                string note = r.ReadString();

                var policy = new SpdfPolicy
                {
                    DocId = docId,
                    MaxPrints = maxPrints,
                    ExpiresAtUtc = expiresTicks > 0 ? new DateTimeOffset(expiresTicks, TimeSpan.Zero) : (DateTimeOffset?)null,
                    MoireLevel = moire,
                    SecurityFlags = flags,
                    OriginalFileName = fileName,
                    WatermarkNote = note
                };

                return (docKey, policy);
            }
        }
    }
}
```


## `src/SecurePdf.Core/Licensing/DeviceAndNetworkContext.cs`


**模块说明**：采集机器 GUID、BIOS UUID、系统卷序列号生成机器指纹码，并获取当前活跃主网卡 IPv4 地址（共 `171` 行）


```csharp
using System;
using System.Collections.Generic;
using System.Linq;
using System.Management;
using System.Net.NetworkInformation;
using System.Net.Sockets;
using System.Runtime.InteropServices;
using System.Text;
using Microsoft.Win32;
using SecurePdf.Core.Crypto;

namespace SecurePdf.Core.Licensing
{
    public sealed class RuntimeTraceContext
    {
        public string MachineCode { get; set; } = string.Empty;
        public byte[] MachineHash { get; set; } = Array.Empty<byte>();
        public string LocalIpAddress { get; set; } = "127.0.0.1";
        public string HostName { get; set; } = Environment.MachineName;
        public string UserName { get; set; } = Environment.UserName;
    }

    public static class DeviceAndNetworkContext
    {
        private const string Alphabet = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern bool GetVolumeInformation(
            string lpRootPathName,
            StringBuilder? lpVolumeNameBuffer,
            int nVolumeNameSize,
            out uint lpVolumeSerialNumber,
            out uint lpMaximumComponentLength,
            out uint lpFileSystemFlags,
            StringBuilder? lpFileSystemNameBuffer,
            int nFileSystemNameSize);

        public static RuntimeTraceContext Capture()
        {
            byte[] machineHash = ComputeMachineHash();
            return new RuntimeTraceContext
            {
                MachineHash = machineHash,
                MachineCode = FormatMachineCode(machineHash),
                LocalIpAddress = GetPrimaryLocalIpAddress(),
                HostName = Environment.MachineName,
                UserName = Environment.UserName
            };
        }

        public static byte[] ComputeMachineHash()
        {
            string machineGuid = ReadRegistryMachineGuid();
            string biosUuid = ReadBiosUuid();
            string volumeSerial = ReadSystemVolumeSerial();

            string raw = $"MG={machineGuid}|BIOS={biosUuid}|VOL={volumeSerial}|OS={Environment.MachineName}";
            return CodeAndCryptoService.ComputeSha256(Encoding.UTF8.GetBytes(raw));
        }

        public static string FormatMachineCode(byte[] machineHash)
        {
            if (machineHash == null || machineHash.Length < 12)
            {
                return "UNKNOWN-DEVICE";
            }

            var sb = new StringBuilder(14);
            for (int i = 0; i < 12; i++)
            {
                if (i > 0 && i % 4 == 0)
                {
                    sb.Append('-');
                }
                sb.Append(Alphabet[machineHash[i] & 0x1F]);
            }
            return sb.ToString();
        }

        public static string GetPrimaryLocalIpAddress()
        {
            try
            {
                var candidates = new List<(int Priority, string Ip)>();
                foreach (NetworkInterface ni in NetworkInterface.GetAllNetworkInterfaces())
                {
                    if (ni.OperationalStatus != OperationalStatus.Up) continue;
                    if (ni.NetworkInterfaceType == NetworkInterfaceType.Loopback ||
                        ni.NetworkInterfaceType == NetworkInterfaceType.Tunnel) continue;

                    IPInterfaceProperties props = ni.GetIPProperties();
                    bool hasGateway = props.GatewayAddresses.Any(g =>
                        g.Address.AddressFamily == AddressFamily.InterNetwork &&
                        !g.Address.ToString().Equals("0.0.0.0", StringComparison.Ordinal));

                    foreach (UnicastIPAddressInformation ua in props.UnicastAddresses)
                    {
                        if (ua.Address.AddressFamily != AddressFamily.InterNetwork) continue;
                        string ip = ua.Address.ToString();
                        if (ip.StartsWith("169.254.", StringComparison.Ordinal)) continue;

                        int priority = hasGateway ? 0 : 1;
                        if (ni.Description.IndexOf("Virtual", StringComparison.OrdinalIgnoreCase) >= 0 ||
                            ni.Description.IndexOf("WSL", StringComparison.OrdinalIgnoreCase) >= 0 ||
                            ni.Description.IndexOf("Hyper-V", StringComparison.OrdinalIgnoreCase) >= 0 ||
                            ni.Description.IndexOf("VMware", StringComparison.OrdinalIgnoreCase) >= 0)
                        {
                            priority += 5;
                        }
                        candidates.Add((priority, ip));
                    }
                }

                if (candidates.Count > 0)
                {
                    return candidates.OrderBy(c => c.Priority).First().Ip;
                }
            }
            catch { }

            return "127.0.0.1";
        }

        private static string ReadRegistryMachineGuid()
        {
            try
            {
                using (var baseKey = RegistryKey.OpenBaseKey(RegistryHive.LocalMachine, RegistryView.Registry64))
                using (var subKey = baseKey.OpenSubKey(@"SOFTWARE\Microsoft\Cryptography"))
                {
                    object? val = subKey?.GetValue("MachineGuid");
                    if (val != null) return val.ToString() ?? string.Empty;
                }
            }
            catch { }
            return string.Empty;
        }

        private static string ReadBiosUuid()
        {
            try
            {
                using (var searcher = new ManagementObjectSearcher("SELECT UUID FROM Win32_ComputerSystemProduct"))
                {
                    foreach (ManagementBaseObject obj in searcher.Get())
                    {
                        string? uuid = obj["UUID"]?.ToString();
                        if (!string.IsNullOrWhiteSpace(uuid)) return uuid!;
                    }
                }
            }
            catch { }
            return string.Empty;
        }

        private static string ReadSystemVolumeSerial()
        {
            try
            {
                string root = Environment.GetFolderPath(Environment.SpecialFolder.Windows);
                string driveRoot = string.IsNullOrEmpty(root) ? "C:\\" : System.IO.Path.GetPathRoot(root) ?? "C:\\";
                if (GetVolumeInformation(driveRoot, null, 0, out uint serial, out _, out _, null, 0))
                {
                    return serial.ToString("X8");
                }
            }
            catch { }
            return string.Empty;
        }
    }
}
```


## `src/SecurePdf.Core/Storage/LocalQuotaStore.cs`


**模块说明**：打印次数与过期时间的 DPAPI + 注册表双存储状态机，含 5 分钟容差的系统时间回拨锁定（共 `356` 行）


```csharp
using System;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using Microsoft.Win32;
using SecurePdf.Core.Crypto;

namespace SecurePdf.Core.Storage
{
    public sealed class QuotaSnapshot
    {
        public Guid DocId { get; set; }
        public int MaxPrints { get; set; }
        public int UsedPrints { get; set; }
        public long SequenceNumber { get; set; }
        public long MaxSeenUtcTicks { get; set; }

        public int RemainingPrints => Math.Max(0, MaxPrints - UsedPrints);
    }

    public sealed class QuotaAccessResult
    {
        public bool IsAllowed { get; set; }
        public string BlockReason { get; set; } = string.Empty;
        public QuotaSnapshot Snapshot { get; set; } = new QuotaSnapshot();
        public bool TamperWarning { get; set; }
    }

    public interface IQuotaPersistenceBackend
    {
        byte[]? ReadPrimary(Guid docId);
        byte[]? ReadSecondary(Guid docId);
        void WritePrimary(Guid docId, byte[] protectedBytes);
        void WriteSecondary(Guid docId, byte[] protectedBytes);
    }

    public sealed class WindowsDualQuotaBackend : IQuotaPersistenceBackend
    {
        private readonly string _directoryPath;
        private readonly string _registryBasePath;

        public WindowsDualQuotaBackend(
            string? directoryPath = null,
            string registryBasePath = @"Software\SecurePdfSuite\Quotas")
        {
            _directoryPath = directoryPath ?? Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "SecurePdfSuite",
                "Quotas");
            _registryBasePath = registryBasePath;
        }

        public byte[]? ReadPrimary(Guid docId)
        {
            string file = Path.Combine(_directoryPath, docId.ToString("N") + ".dat");
            return File.Exists(file) ? File.ReadAllBytes(file) : null;
        }

        public byte[]? ReadSecondary(Guid docId)
        {
            using (RegistryKey? key = Registry.CurrentUser.OpenSubKey(_registryBasePath, writable: false))
            {
                return key?.GetValue(docId.ToString("N")) as byte[];
            }
        }

        public void WritePrimary(Guid docId, byte[] protectedBytes)
        {
            Directory.CreateDirectory(_directoryPath);
            string file = Path.Combine(_directoryPath, docId.ToString("N") + ".dat");
            File.WriteAllBytes(file, protectedBytes);
        }

        public void WriteSecondary(Guid docId, byte[] protectedBytes)
        {
            using (RegistryKey key = Registry.CurrentUser.CreateSubKey(_registryBasePath))
            {
                key.SetValue(docId.ToString("N"), protectedBytes, RegistryValueKind.Binary);
            }
        }
    }

    public sealed class LocalQuotaStore
    {
        private static readonly TimeSpan ClockRollbackTolerance = TimeSpan.FromMinutes(5);
        private static readonly byte[] AppEntropy = Encoding.UTF8.GetBytes("SecurePdfSuite-Quota-v2");

        private readonly IQuotaPersistenceBackend _backend;
        private readonly byte[] _machineHash;
        private readonly bool _useDpapi;

        public LocalQuotaStore(
            byte[] machineHash,
            IQuotaPersistenceBackend? backend = null,
            bool useDpapi = true)
        {
            _machineHash = machineHash ?? throw new ArgumentNullException(nameof(machineHash));
            _backend = backend ?? new WindowsDualQuotaBackend();
            _useDpapi = useDpapi;
        }

        public QuotaAccessResult LoadAndVerify(
            Guid docId,
            int policyMaxPrints,
            DateTimeOffset? expiresAtUtc,
            DateTimeOffset? currentUtcNow = null)
        {
            DateTimeOffset now = currentUtcNow ?? DateTimeOffset.UtcNow;

            QuotaSnapshot? primary = TryDecode(_backend.ReadPrimary(docId), docId, out bool primaryCorrupt);
            QuotaSnapshot? secondary = TryDecode(_backend.ReadSecondary(docId), docId, out bool secondaryCorrupt);

            bool tamperDetected = primaryCorrupt || secondaryCorrupt;
            QuotaSnapshot effective;

            if (primary == null && secondary == null)
            {
                if (tamperDetected)
                {
                    return new QuotaAccessResult
                    {
                        IsAllowed = false,
                        TamperWarning = true,
                        BlockReason = "检测到本地授权状态文件被破坏，已锁定文档访问。",
                        Snapshot = new QuotaSnapshot { DocId = docId, MaxPrints = policyMaxPrints, UsedPrints = policyMaxPrints }
                    };
                }

                effective = new QuotaSnapshot
                {
                    DocId = docId,
                    MaxPrints = policyMaxPrints,
                    UsedPrints = 0,
                    SequenceNumber = 1,
                    MaxSeenUtcTicks = now.UtcTicks
                };
            }
            else if (primary != null && secondary != null)
            {
                if (primary.UsedPrints != secondary.UsedPrints ||
                    primary.SequenceNumber != secondary.SequenceNumber)
                {
                    tamperDetected = true;
                }

                effective = new QuotaSnapshot
                {
                    DocId = docId,
                    MaxPrints = Math.Min(policyMaxPrints, Math.Min(primary.MaxPrints, secondary.MaxPrints)),
                    UsedPrints = Math.Max(primary.UsedPrints, secondary.UsedPrints),
                    SequenceNumber = Math.Max(primary.SequenceNumber, secondary.SequenceNumber) + 1,
                    MaxSeenUtcTicks = Math.Max(primary.MaxSeenUtcTicks, secondary.MaxSeenUtcTicks)
                };
            }
            else
            {
                tamperDetected = true;
                QuotaSnapshot survivor = primary ?? secondary!;
                effective = new QuotaSnapshot
                {
                    DocId = docId,
                    MaxPrints = Math.Min(policyMaxPrints, survivor.MaxPrints),
                    UsedPrints = survivor.UsedPrints,
                    SequenceNumber = survivor.SequenceNumber + 1,
                    MaxSeenUtcTicks = survivor.MaxSeenUtcTicks
                };
            }

            if (now.UtcTicks + ClockRollbackTolerance.Ticks < effective.MaxSeenUtcTicks)
            {
                return new QuotaAccessResult
                {
                    IsAllowed = false,
                    TamperWarning = true,
                    BlockReason = "检测到系统时间被回拨，文档已被安全锁定。请校准系统时间后重试。",
                    Snapshot = effective
                };
            }

            if (now.UtcTicks > effective.MaxSeenUtcTicks)
            {
                effective.MaxSeenUtcTicks = now.UtcTicks;
            }

            SaveBoth(effective);

            if (expiresAtUtc.HasValue && now > expiresAtUtc.Value)
            {
                return new QuotaAccessResult
                {
                    IsAllowed = false,
                    TamperWarning = tamperDetected,
                    BlockReason = $"文档授权已过期（有效期至 {expiresAtUtc.Value.ToLocalTime():yyyy-MM-dd HH:mm}）。",
                    Snapshot = effective
                };
            }

            return new QuotaAccessResult
            {
                IsAllowed = true,
                TamperWarning = tamperDetected,
                Snapshot = effective
            };
        }

        public QuotaSnapshot ReservePrintSlot(
            Guid docId,
            int policyMaxPrints,
            DateTimeOffset? expiresAtUtc,
            DateTimeOffset? currentUtcNow = null)
        {
            QuotaAccessResult status = LoadAndVerify(docId, policyMaxPrints, expiresAtUtc, currentUtcNow);
            if (!status.IsAllowed)
            {
                throw new InvalidOperationException(status.BlockReason);
            }

            if (status.Snapshot.RemainingPrints <= 0)
            {
                throw new InvalidOperationException("已达到最大允许打印次数，禁止继续打印。");
            }

            QuotaSnapshot updated = new QuotaSnapshot
            {
                DocId = docId,
                MaxPrints = status.Snapshot.MaxPrints,
                UsedPrints = status.Snapshot.UsedPrints + 1,
                SequenceNumber = status.Snapshot.SequenceNumber + 1,
                MaxSeenUtcTicks = Math.Max(status.Snapshot.MaxSeenUtcTicks, (currentUtcNow ?? DateTimeOffset.UtcNow).UtcTicks)
            };

            SaveBoth(updated);
            return updated;
        }

        public QuotaSnapshot RollbackPrintSlot(QuotaSnapshot reservedSnapshot)
        {
            if (reservedSnapshot == null)
            {
                throw new ArgumentNullException(nameof(reservedSnapshot));
            }

            QuotaSnapshot rolledBack = new QuotaSnapshot
            {
                DocId = reservedSnapshot.DocId,
                MaxPrints = reservedSnapshot.MaxPrints,
                UsedPrints = Math.Max(0, reservedSnapshot.UsedPrints - 1),
                SequenceNumber = reservedSnapshot.SequenceNumber + 1,
                MaxSeenUtcTicks = reservedSnapshot.MaxSeenUtcTicks
            };

            SaveBoth(rolledBack);
            return rolledBack;
        }

        private void SaveBoth(QuotaSnapshot snapshot)
        {
            byte[] encoded = Encode(snapshot);
            _backend.WritePrimary(snapshot.DocId, encoded);
            _backend.WriteSecondary(snapshot.DocId, encoded);
        }

        private byte[] Encode(QuotaSnapshot snapshot)
        {
            byte[] payload;
            using (var ms = new MemoryStream(64))
            using (var w = new BinaryWriter(ms))
            {
                w.Write((byte)1);
                w.Write(snapshot.DocId.ToByteArray());
                w.Write(snapshot.MaxPrints);
                w.Write(snapshot.UsedPrints);
                w.Write(snapshot.SequenceNumber);
                w.Write(snapshot.MaxSeenUtcTicks);
                payload = ms.ToArray();
            }

            byte[] mac = CodeAndCryptoService.ComputeHmacSha256(_machineHash, payload);
            byte[] combined = new byte[payload.Length + mac.Length];
            Buffer.BlockCopy(payload, 0, combined, 0, payload.Length);
            Buffer.BlockCopy(mac, 0, combined, payload.Length, mac.Length);

            return _useDpapi
                ? ProtectedData.Protect(combined, AppEntropy, DataProtectionScope.CurrentUser)
                : combined;
        }

        private QuotaSnapshot? TryDecode(byte[]? blob, Guid expectedDocId, out bool isCorrupt)
        {
            isCorrupt = false;
            if (blob == null || blob.Length == 0)
            {
                return null;
            }

            try
            {
                byte[] combined = _useDpapi
                    ? ProtectedData.Unprotect(blob, AppEntropy, DataProtectionScope.CurrentUser)
                    : blob;

                const int macLen = 32;
                if (combined.Length <= macLen + 20)
                {
                    isCorrupt = true;
                    return null;
                }

                int payloadLen = combined.Length - macLen;
                byte[] payload = new byte[payloadLen];
                byte[] actualMac = new byte[macLen];
                Buffer.BlockCopy(combined, 0, payload, 0, payloadLen);
                Buffer.BlockCopy(combined, payloadLen, actualMac, 0, macLen);

                byte[] expectedMac = CodeAndCryptoService.ComputeHmacSha256(_machineHash, payload);
                if (!CodeAndCryptoService.FixedTimeEquals(actualMac, expectedMac))
                {
                    isCorrupt = true;
                    return null;
                }

                using (var ms = new MemoryStream(payload))
                using (var r = new BinaryReader(ms))
                {
                    byte ver = r.ReadByte();
                    if (ver != 1)
                    {
                        isCorrupt = true;
                        return null;
                    }

                    Guid docId = new Guid(r.ReadBytes(16));
                    if (docId != expectedDocId)
                    {
                        isCorrupt = true;
                        return null;
                    }

                    return new QuotaSnapshot
                    {
                        DocId = docId,
                        MaxPrints = r.ReadInt32(),
                        UsedPrints = r.ReadInt32(),
                        SequenceNumber = r.ReadInt64(),
                        MaxSeenUtcTicks = r.ReadInt64()
                    };
                }
            }
            catch
            {
                isCorrupt = true;
                return null;
            }
        }
    }
}
```


---

[返回总览 README](../README.md)
