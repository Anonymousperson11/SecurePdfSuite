# 01 — 核心加密、`.spdf` 容器格式与防篡改配额

> [!NOTE]
> 本文档对应核心库 `SecurePdf.Core` 的构建配置、密码学实现、`.spdf` v2 容器封装、设备指纹与双存储配额状态机（v1.1.0）。
> v1.1.0 移除了 `BouncyCastle.Cryptography` 外部依赖，改用自包含 RFC 7914 `scrypt` 与 Windows 原生 CNG (`bcrypt.dll`) AES-256-GCM，同时保持与 v1.0.x `.spdf` 文件字节级兼容。

## 目录

- [`Directory.Build.props`](#directorybuildprops)
- [`src/SecurePdf.Core/SecurePdf.Core.csproj`](#srcsecurepdfcoresecurepdfcorecsproj)
- [`src/SecurePdf.Core/Crypto/CodeAndCryptoService.cs`](#srcsecurepdfcorecryptocodeandcryptoservicecs)
- [`src/SecurePdf.Core/Format/SpdfContainer.cs`](#srcsecurepdfcoreformatspdfcontainercs)
- [`src/SecurePdf.Core/Licensing/DeviceAndNetworkContext.cs`](#srcsecurepdfcorelicensingdeviceandnetworkcontextcs)
- [`src/SecurePdf.Core/Storage/LocalQuotaStore.cs`](#srcsecurepdfcorestoragelocalquotastorecs)

## `Directory.Build.props`

**模块说明**：全局 MSBuild 构建属性（`net48` 目标框架、`x64` 目标平台、C# 12、启用可空注解、确定性编译、警告视为错误及统一版本号 `1.1.0`）（共 `23` 行）

```xml
<Project>
  <PropertyGroup>
    <LangVersion>12.0</LangVersion>
    <Nullable>enable</Nullable>
    <ImplicitUsings>false</ImplicitUsings>
    <PlatformTarget>x64</PlatformTarget>
    <Deterministic>true</Deterministic>
    <TreatWarningsAsErrors>true</TreatWarningsAsErrors>
    <Version>1.1.0</Version>
    <Company>SecurePdfSuite</Company>
    <Product>SecurePdfSuite</Product>
    <NeutralLanguage>zh-CN</NeutralLanguage>
    <SatelliteResourceLanguages>zh-Hans</SatelliteResourceLanguages>
    <GenerateDocumentationFile>false</GenerateDocumentationFile>
  </PropertyGroup>

  <!-- 发布构建不生成 PDB、不带源码路径信息 -->
  <PropertyGroup Condition="'$(Configuration)' == 'Release'">
    <DebugType>none</DebugType>
    <DebugSymbols>false</DebugSymbols>
    <Optimize>true</Optimize>
  </PropertyGroup>
</Project>
```

## `src/SecurePdf.Core/SecurePdf.Core.csproj`

**模块说明**：核心类库项目配置（零第三方运行期 NuGet 依赖，启用不安全代码用于高性能像素/指针操作，仅引用系统程序集）（共 `17` 行）

```xml
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net48</TargetFramework>
    <AllowUnsafeBlocks>true</AllowUnsafeBlocks>
  </PropertyGroup>

  <ItemGroup>
    <PackageReference Include="Microsoft.NETFramework.ReferenceAssemblies.net48" Version="1.0.3" PrivateAssets="All" />
  </ItemGroup>

  <!-- 仅引用 .NET Framework 自带程序集：加密走 Windows CNG，不再依赖 BouncyCastle -->
  <ItemGroup>
    <Reference Include="System.Drawing" />
    <Reference Include="System.Management" />
    <Reference Include="System.Security" />
  </ItemGroup>
</Project>
```

## `src/SecurePdf.Core/Crypto/CodeAndCryptoService.cs`

**模块说明**：剔除易混淆字符的随机验证码生成、全角/空白/连字符规范化 (`NormalizeCode`)、纯托管 RFC 7914 `scrypt` 密钥派生（含成本上限护栏）、Windows CNG `bcrypt.dll` AES-256-GCM 加解密、HMAC-SHA256 与内存清零（共 `447` 行）

```csharp
using System;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;

namespace SecurePdf.Core.Crypto
{
    /// <summary>
    /// 验证码生成与全部密码学原语。
    /// 不依赖任何第三方库：AES-256-GCM 走 Windows CNG (bcrypt.dll，自带 AES-NI 加速)，
    /// scrypt 按 RFC 7914 自行实现，HMAC/SHA-256 使用 .NET Framework 自带实现。
    /// 输出与 v1.0.x（BouncyCastle 实现）逐字节一致，新旧 .spdf 文件双向兼容。
    /// </summary>
    public static class CodeAndCryptoService
    {
        private const string CodeAlphabet = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

        /// <summary>GCM 认证标签长度（字节），密文布局为 ciphertext || tag。</summary>
        public const int GcmTagSize = 16;

        public static string GenerateVerificationCode(int groupCount = 3, int groupLength = 4)
        {
            if (groupCount <= 0) throw new ArgumentOutOfRangeException(nameof(groupCount));
            if (groupLength <= 0) throw new ArgumentOutOfRangeException(nameof(groupLength));

            int totalChars = groupCount * groupLength;
            byte[] randomBytes = GenerateRandomBytes(totalChars);
            var sb = new StringBuilder(totalChars + groupCount - 1);

            for (int i = 0; i < totalChars; i++)
            {
                if (i > 0 && i % groupLength == 0) sb.Append('-');
                // 256 % 32 == 0，按位取低 5 位不存在取模偏差
                sb.Append(CodeAlphabet[randomBytes[i] & 0x1F]);
            }

            ZeroMemory(randomBytes);
            return sb.ToString();
        }

        /// <summary>
        /// 归一化用户输入的验证码：去掉空白与各类连字符、全角转半角、统一大写。
        /// 对自动生成的验证码（仅含 A-Z / 2-9）结果与旧版完全一致。
        /// </summary>
        public static string NormalizeCode(string? code)
        {
            if (string.IsNullOrWhiteSpace(code)) return string.Empty;

            var sb = new StringBuilder(code!.Length);
            foreach (char raw in code)
            {
                char ch = raw;
                if (ch >= '！' && ch <= '～') ch = (char)(ch - 0xFEE0);   // 全角 ASCII -> 半角
                if (char.IsWhiteSpace(ch) || ch == '-' || ch == '　') continue;
                if (ch >= '‐' && ch <= '―') continue;                    // ‐ ‑ ‒ – — ―
                if (ch == '−' || ch == '_') continue;                         // 数学减号 / 下划线
                sb.Append(char.ToUpperInvariant(ch));
            }
            return sb.ToString();
        }

        public static byte[] GenerateRandomBytes(int length)
        {
            byte[] data = new byte[length];
            using (var rng = RandomNumberGenerator.Create())
            {
                rng.GetBytes(data);
            }
            return data;
        }

        // ------------------------------------------------------------------ scrypt (RFC 7914)

        /// <summary>校验 scrypt 参数是否在本软件接受的范围内（同时约束内存与耗时，防止恶意文件头耗尽资源）。</summary>
        public static bool IsAcceptableScryptCost(int costN, int blockSizeR, int parallelP)
        {
            if (costN < 2 || costN > 1048576 || (costN & (costN - 1)) != 0) return false;
            if (blockSizeR < 1 || blockSizeR > 32) return false;
            if (parallelP < 1 || parallelP > 16) return false;
            // 内存占用 = 128 * N * r 字节，上限 128 MB；总工作量 N * r * p 同步设上限，避免恶意文件头让解锁卡死
            long memoryUnits = (long)costN * blockSizeR;
            return memoryUnits <= 1048576L && memoryUnits * parallelP <= 4194304L;
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
                throw new ArgumentException("验证码不能为空。", nameof(normalizedCode));
            }

            byte[] passwordBytes = Encoding.UTF8.GetBytes(normalizedCode);
            try
            {
                return Scrypt(passwordBytes, salt, costN, blockSizeR, parallelP, outputLength);
            }
            finally
            {
                ZeroMemory(passwordBytes);
            }
        }

        public static byte[] Scrypt(byte[] password, byte[] salt, int costN, int blockSizeR, int parallelP, int outputLength)
        {
            if (password == null) throw new ArgumentNullException(nameof(password));
            if (salt == null) throw new ArgumentNullException(nameof(salt));
            if (outputLength <= 0) throw new ArgumentOutOfRangeException(nameof(outputLength));
            if (!IsAcceptableScryptCost(costN, blockSizeR, parallelP))
            {
                throw new ArgumentOutOfRangeException(nameof(costN), "scrypt 参数超出允许范围。");
            }

            int blockWords = 32 * blockSizeR;            // 128 * r 字节 = 32 * r 个 uint
            int blockBytes = blockWords * 4;

            byte[] b = Pbkdf2Sha256SingleIteration(password, salt, parallelP * blockBytes);
            uint[] v = new uint[costN * blockWords];
            uint[] x = new uint[blockWords];
            uint[] y = new uint[blockWords];
            try
            {
                for (int i = 0; i < parallelP; i++)
                {
                    Buffer.BlockCopy(b, i * blockBytes, x, 0, blockBytes);
                    RoMix(x, y, v, costN, blockSizeR);
                    Buffer.BlockCopy(x, 0, b, i * blockBytes, blockBytes);
                }
                return Pbkdf2Sha256SingleIteration(password, b, outputLength);
            }
            finally
            {
                Array.Clear(v, 0, v.Length);
                Array.Clear(x, 0, x.Length);
                Array.Clear(y, 0, y.Length);
                ZeroMemory(b);
            }
        }

        private static void RoMix(uint[] x, uint[] y, uint[] v, int n, int r)
        {
            int blockWords = 32 * r;
            int lastBlock = (2 * r - 1) * 16;
            int mask = n - 1;

            for (int i = 0; i < n; i++)
            {
                Buffer.BlockCopy(x, 0, v, i * blockWords * 4, blockWords * 4);
                BlockMix(x, y, r);
            }

            for (int i = 0; i < n; i++)
            {
                int j = (int)(x[lastBlock] & (uint)mask) * blockWords;
                for (int k = 0; k < x.Length; k++) x[k] ^= v[j + k];
                BlockMix(x, y, r);
            }
        }

        /// <summary>scryptBlockMix：结果写回 <paramref name="b"/>，<paramref name="y"/> 为临时缓冲。</summary>
        private static void BlockMix(uint[] b, uint[] y, int r)
        {
            int last = (2 * r - 1) * 16;
            uint x0 = b[last], x1 = b[last + 1], x2 = b[last + 2], x3 = b[last + 3];
            uint x4 = b[last + 4], x5 = b[last + 5], x6 = b[last + 6], x7 = b[last + 7];
            uint x8 = b[last + 8], x9 = b[last + 9], x10 = b[last + 10], x11 = b[last + 11];
            uint x12 = b[last + 12], x13 = b[last + 13], x14 = b[last + 14], x15 = b[last + 15];

            for (int i = 0; i < 2 * r; i++)
            {
                int s = i * 16;
                x0 ^= b[s]; x1 ^= b[s + 1]; x2 ^= b[s + 2]; x3 ^= b[s + 3];
                x4 ^= b[s + 4]; x5 ^= b[s + 5]; x6 ^= b[s + 6]; x7 ^= b[s + 7];
                x8 ^= b[s + 8]; x9 ^= b[s + 9]; x10 ^= b[s + 10]; x11 ^= b[s + 11];
                x12 ^= b[s + 12]; x13 ^= b[s + 13]; x14 ^= b[s + 14]; x15 ^= b[s + 15];

                // Salsa20/8 核心
                uint j0 = x0, j1 = x1, j2 = x2, j3 = x3, j4 = x4, j5 = x5, j6 = x6, j7 = x7;
                uint j8 = x8, j9 = x9, j10 = x10, j11 = x11, j12 = x12, j13 = x13, j14 = x14, j15 = x15;
                uint t;
                for (int round = 0; round < 4; round++)
                {
                    t = x0 + x12; x4 ^= (t << 7) | (t >> 25);
                    t = x4 + x0; x8 ^= (t << 9) | (t >> 23);
                    t = x8 + x4; x12 ^= (t << 13) | (t >> 19);
                    t = x12 + x8; x0 ^= (t << 18) | (t >> 14);
                    t = x5 + x1; x9 ^= (t << 7) | (t >> 25);
                    t = x9 + x5; x13 ^= (t << 9) | (t >> 23);
                    t = x13 + x9; x1 ^= (t << 13) | (t >> 19);
                    t = x1 + x13; x5 ^= (t << 18) | (t >> 14);
                    t = x10 + x6; x14 ^= (t << 7) | (t >> 25);
                    t = x14 + x10; x2 ^= (t << 9) | (t >> 23);
                    t = x2 + x14; x6 ^= (t << 13) | (t >> 19);
                    t = x6 + x2; x10 ^= (t << 18) | (t >> 14);
                    t = x15 + x11; x3 ^= (t << 7) | (t >> 25);
                    t = x3 + x15; x7 ^= (t << 9) | (t >> 23);
                    t = x7 + x3; x11 ^= (t << 13) | (t >> 19);
                    t = x11 + x7; x15 ^= (t << 18) | (t >> 14);

                    t = x0 + x3; x1 ^= (t << 7) | (t >> 25);
                    t = x1 + x0; x2 ^= (t << 9) | (t >> 23);
                    t = x2 + x1; x3 ^= (t << 13) | (t >> 19);
                    t = x3 + x2; x0 ^= (t << 18) | (t >> 14);
                    t = x5 + x4; x6 ^= (t << 7) | (t >> 25);
                    t = x6 + x5; x7 ^= (t << 9) | (t >> 23);
                    t = x7 + x6; x4 ^= (t << 13) | (t >> 19);
                    t = x4 + x7; x5 ^= (t << 18) | (t >> 14);
                    t = x10 + x9; x11 ^= (t << 7) | (t >> 25);
                    t = x11 + x10; x8 ^= (t << 9) | (t >> 23);
                    t = x8 + x11; x9 ^= (t << 13) | (t >> 19);
                    t = x9 + x8; x10 ^= (t << 18) | (t >> 14);
                    t = x15 + x14; x12 ^= (t << 7) | (t >> 25);
                    t = x12 + x15; x13 ^= (t << 9) | (t >> 23);
                    t = x13 + x12; x14 ^= (t << 13) | (t >> 19);
                    t = x14 + x13; x15 ^= (t << 18) | (t >> 14);
                }
                x0 += j0; x1 += j1; x2 += j2; x3 += j3; x4 += j4; x5 += j5; x6 += j6; x7 += j7;
                x8 += j8; x9 += j9; x10 += j10; x11 += j11; x12 += j12; x13 += j13; x14 += j14; x15 += j15;

                // 偶数块依次放前半段，奇数块依次放后半段
                int d = ((i & 1) == 0 ? i / 2 : r + i / 2) * 16;
                y[d] = x0; y[d + 1] = x1; y[d + 2] = x2; y[d + 3] = x3;
                y[d + 4] = x4; y[d + 5] = x5; y[d + 6] = x6; y[d + 7] = x7;
                y[d + 8] = x8; y[d + 9] = x9; y[d + 10] = x10; y[d + 11] = x11;
                y[d + 12] = x12; y[d + 13] = x13; y[d + 14] = x14; y[d + 15] = x15;
            }

            Buffer.BlockCopy(y, 0, b, 0, b.Length * 4);
        }

        /// <summary>PBKDF2-HMAC-SHA256，迭代次数固定为 1（scrypt 只用到这一种）。</summary>
        private static byte[] Pbkdf2Sha256SingleIteration(byte[] password, byte[] salt, int outputLength)
        {
            byte[] result = new byte[outputLength];
            byte[] block = new byte[salt.Length + 4];
            Buffer.BlockCopy(salt, 0, block, 0, salt.Length);

            using (var hmac = new HMACSHA256(password))
            {
                int offset = 0;
                for (int index = 1; offset < outputLength; index++)
                {
                    block[salt.Length] = (byte)(index >> 24);
                    block[salt.Length + 1] = (byte)(index >> 16);
                    block[salt.Length + 2] = (byte)(index >> 8);
                    block[salt.Length + 3] = (byte)index;

                    byte[] t = hmac.ComputeHash(block);
                    int take = Math.Min(t.Length, outputLength - offset);
                    Buffer.BlockCopy(t, 0, result, offset, take);
                    ZeroMemory(t);
                    offset += take;
                }
            }

            ZeroMemory(block);
            return result;
        }

        // ------------------------------------------------------------------ AES-256-GCM (Windows CNG)

        public static byte[] EncryptAesGcm(byte[] key, byte[] nonce, byte[] plaintext, byte[]? aad = null)
        {
            if (plaintext == null) throw new ArgumentNullException(nameof(plaintext));
            byte[] output = new byte[checked(plaintext.Length + GcmTagSize)];
            if (!AesGcmCng.Transform(true, key, nonce, aad, plaintext, plaintext.Length, output, output, plaintext.Length))
            {
                throw new CryptographicException("AES-GCM 加密失败。");
            }
            return output;
        }

        public static byte[] DecryptAesGcm(byte[] key, byte[] nonce, byte[] ciphertextWithTag, byte[]? aad = null)
        {
            if (ciphertextWithTag == null) throw new ArgumentNullException(nameof(ciphertextWithTag));
            if (ciphertextWithTag.Length < GcmTagSize)
            {
                throw new CryptographicException("密文长度无效。");
            }

            int length = ciphertextWithTag.Length - GcmTagSize;
            byte[] output = new byte[length];
            if (!AesGcmCng.Transform(false, key, nonce, aad, ciphertextWithTag, length, output, ciphertextWithTag, length))
            {
                ZeroMemory(output);
                throw new CryptographicException("验证码不正确，或文件内容已被篡改 / 损坏。");
            }
            return output;
        }

        private static unsafe class AesGcmCng
        {
            private const int StatusAuthTagMismatch = unchecked((int)0xC000A002);

            private static readonly object InitLock = new object();
            private static IntPtr _algorithm;

            [StructLayout(LayoutKind.Sequential)]
            private struct AuthenticatedCipherModeInfo
            {
                public int cbSize;
                public int dwInfoVersion;
                public byte* pbNonce;
                public int cbNonce;
                public byte* pbAuthData;
                public int cbAuthData;
                public byte* pbTag;
                public int cbTag;
                public byte* pbMacContext;
                public int cbMacContext;
                public int cbAAD;
                public long cbData;
                public int dwFlags;
            }

            [DllImport("bcrypt.dll", CharSet = CharSet.Unicode)]
            private static extern int BCryptOpenAlgorithmProvider(out IntPtr phAlgorithm, string pszAlgId, string? pszImplementation, int dwFlags);

            [DllImport("bcrypt.dll", CharSet = CharSet.Unicode)]
            private static extern int BCryptSetProperty(IntPtr hObject, string pszProperty, byte[] pbInput, int cbInput, int dwFlags);

            [DllImport("bcrypt.dll")]
            private static extern int BCryptGenerateSymmetricKey(IntPtr hAlgorithm, out IntPtr phKey, IntPtr pbKeyObject, int cbKeyObject, byte[] pbSecret, int cbSecret, int dwFlags);

            [DllImport("bcrypt.dll")]
            private static extern int BCryptDestroyKey(IntPtr hKey);

            [DllImport("bcrypt.dll")]
            private static extern int BCryptEncrypt(IntPtr hKey, byte* pbInput, int cbInput, ref AuthenticatedCipherModeInfo pPaddingInfo, IntPtr pbIV, int cbIV, byte* pbOutput, int cbOutput, out int pcbResult, int dwFlags);

            [DllImport("bcrypt.dll")]
            private static extern int BCryptDecrypt(IntPtr hKey, byte* pbInput, int cbInput, ref AuthenticatedCipherModeInfo pPaddingInfo, IntPtr pbIV, int cbIV, byte* pbOutput, int cbOutput, out int pcbResult, int dwFlags);

            private static IntPtr GetAlgorithm()
            {
                lock (InitLock)
                {
                    if (_algorithm == IntPtr.Zero)
                    {
                        int status = BCryptOpenAlgorithmProvider(out IntPtr handle, "AES", null, 0);
                        if (status != 0) throw new CryptographicException($"无法初始化系统 AES 算法 (0x{status:X8})。");

                        byte[] mode = Encoding.Unicode.GetBytes("ChainingModeGCM\0");
                        status = BCryptSetProperty(handle, "ChainingMode", mode, mode.Length, 0);
                        if (status != 0) throw new CryptographicException($"系统不支持 AES-GCM (0x{status:X8})。");

                        _algorithm = handle;   // 进程生命周期内复用，不关闭
                    }
                    return _algorithm;
                }
            }

            /// <summary>
            /// 加密：input[0..length) -> output[0..length)，标签写入 tagBuffer[tagOffset..+16)。
            /// 解密：input[0..length) -> output[0..length)，标签取自 tagBuffer[tagOffset..+16)；认证失败返回 false。
            /// </summary>
            public static bool Transform(bool encrypt, byte[] key, byte[] nonce, byte[]? aad, byte[] input, int length, byte[] output, byte[] tagBuffer, int tagOffset)
            {
                if (key == null) throw new ArgumentNullException(nameof(key));
                if (nonce == null) throw new ArgumentNullException(nameof(nonce));
                if (key.Length != 16 && key.Length != 24 && key.Length != 32) throw new CryptographicException("AES 密钥长度无效。");
                if (nonce.Length != 12) throw new CryptographicException("GCM Nonce 必须为 12 字节。");

                int status = BCryptGenerateSymmetricKey(GetAlgorithm(), out IntPtr hKey, IntPtr.Zero, 0, key, key.Length, 0);
                if (status != 0) throw new CryptographicException($"无法导入 AES 密钥 (0x{status:X8})。");

                try
                {
                    fixed (byte* pNonce = nonce)
                    fixed (byte* pAad = aad)
                    fixed (byte* pIn = input)
                    fixed (byte* pOut = output)
                    fixed (byte* pTag = tagBuffer)
                    {
                        var info = new AuthenticatedCipherModeInfo
                        {
                            cbSize = sizeof(AuthenticatedCipherModeInfo),
                            dwInfoVersion = 1,
                            pbNonce = pNonce,
                            cbNonce = nonce.Length,
                            pbAuthData = aad == null || aad.Length == 0 ? null : pAad,
                            cbAuthData = aad == null ? 0 : aad.Length,
                            pbTag = pTag + tagOffset,
                            cbTag = GcmTagSize
                        };

                        byte* src = length == 0 ? null : pIn;
                        byte* dst = length == 0 ? null : pOut;
                        status = encrypt
                            ? BCryptEncrypt(hKey, src, length, ref info, IntPtr.Zero, 0, dst, length, out int written, 0)
                            : BCryptDecrypt(hKey, src, length, ref info, IntPtr.Zero, 0, dst, length, out written, 0);

                        if (status == StatusAuthTagMismatch) return false;
                        if (status != 0) throw new CryptographicException($"AES-GCM 运算失败 (0x{status:X8})。");
                        if (written != length) throw new CryptographicException("AES-GCM 输出长度异常。");
                        return true;
                    }
                }
                finally
                {
                    BCryptDestroyKey(hKey);
                }
            }
        }

        // ------------------------------------------------------------------ 哈希 / 常量时间比较 / 内存擦除

        public static byte[] ComputeSha256(byte[] data)
        {
            using (var sha = SHA256.Create())
            {
                return sha.ComputeHash(data);
            }
        }

        public static byte[] ComputeHmacSha256(byte[] key, byte[] data)
        {
            using (var hmac = new HMACSHA256(key))
            {
                return hmac.ComputeHash(data);
            }
        }

        [MethodImpl(MethodImplOptions.NoInlining | MethodImplOptions.NoOptimization)]
        public static bool FixedTimeEquals(byte[]? a, byte[]? b)
        {
            if (a == null || b == null || a.Length != b.Length) return false;

            int diff = 0;
            for (int i = 0; i < a.Length; i++) diff |= a[i] ^ b[i];
            return diff == 0;
        }

        [MethodImpl(MethodImplOptions.NoInlining | MethodImplOptions.NoOptimization)]
        public static void ZeroMemory(byte[]? buffer)
        {
            if (buffer == null) return;
            for (int i = 0; i < buffer.Length; i++) buffer[i] = 0;
        }
    }
}
```

## `src/SecurePdf.Core/Format/SpdfContainer.cs`

**模块说明**：`.spdf` v2 二进制文件头定义、1024 字节 PDF 魔数识别、`MaxPdfBytes = 1 GiB` 与截断/成本边界校验、AAD 头部策略绑定、`.tmp` 原子替换写入及明文缓冲区擦除（共 `487` 行）

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
            if (_disposed) return;
            _disposed = true;
            CodeAndCryptoService.ZeroMemory(_pdfBytes);
            _pdfBytes = null;
        }
    }

    /// <summary>
    /// .spdf v2 容器：
    /// <code>
    /// "SPDF" | version(u16) | docId(16) | salt(16) | N(i32) | r(i32) | p(i32)      ← 以上 50 字节同时作为两段 GCM 的 AAD
    /// | wrapNonce(12) | wrappedLen(i32) | AES-GCM(KEK = scrypt(验证码), 策略 + 文档密钥)
    /// | payloadNonce(12) | payloadLen(i64) | AES-GCM(文档密钥, 原始 PDF)
    /// </code>
    /// </summary>
    public static class SpdfContainer
    {
        private static readonly byte[] MagicBytes = { (byte)'S', (byte)'P', (byte)'D', (byte)'F' };
        private const ushort CurrentVersion = 2;
        private const int SaltSize = 16;
        private const int NonceSize = 12;
        private const int DocKeySize = 32;
        private const int TagSize = CodeAndCryptoService.GcmTagSize;
        private const int MaxPolicyBlockBytes = 1024 * 1024;
        private const int PdfHeaderSearchWindow = 1024;

        /// <summary>单个 PDF 的体积上限（.NET Framework 单个数组上限约 2 GB，留出加解密副本余量）。</summary>
        public const long MaxPdfBytes = 1024L * 1024 * 1024;

        /// <summary>按 PDF 规范，"%PDF-" 文件头允许出现在前 1024 字节内（部分导出工具会在前面带 BOM 或空行）。</summary>
        public static bool LooksLikePdf(byte[]? data)
        {
            if (data == null) return false;
            int limit = Math.Min(data.Length, PdfHeaderSearchWindow) - 5;
            for (int i = 0; i <= limit; i++)
            {
                if (data[i] == '%' && data[i + 1] == 'P' && data[i + 2] == 'D' && data[i + 3] == 'F' && data[i + 4] == '-')
                {
                    return true;
                }
            }
            return false;
        }

        public static (Guid DocId, string VerificationCode) Create(
            byte[] rawPdfBytes,
            string outputSpdfPath,
            SpdfCreationOptions options)
        {
            if (rawPdfBytes == null || rawPdfBytes.Length < 8)
            {
                throw new ArgumentException("PDF 内容为空或无效。", nameof(rawPdfBytes));
            }
            if (string.IsNullOrWhiteSpace(outputSpdfPath))
            {
                throw new ArgumentException("输出路径不能为空。", nameof(outputSpdfPath));
            }
            if (options == null) throw new ArgumentNullException(nameof(options));
            if (options.MaxPrints < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(options), "允许打印次数不能为负数。");
            }
            if (rawPdfBytes.LongLength > MaxPdfBytes)
            {
                throw new InvalidDataException("PDF 文件超过 1 GB，无法处理。");
            }
            if (!LooksLikePdf(rawPdfBytes))
            {
                throw new InvalidDataException("源文件不是有效的 PDF（缺少 %PDF 文件头）。");
            }
            if (!CodeAndCryptoService.IsAcceptableScryptCost(options.KdfCostN, options.KdfBlockSizeR, options.KdfParallelP))
            {
                throw new ArgumentOutOfRangeException(nameof(options), "密钥派生参数超出允许范围。");
            }

            string verificationCode = string.IsNullOrWhiteSpace(options.CustomVerificationCode)
                ? CodeAndCryptoService.GenerateVerificationCode()
                : options.CustomVerificationCode!.Trim().ToUpperInvariant();

            string normalizedCode = CodeAndCryptoService.NormalizeCode(verificationCode);
            if (normalizedCode.Length < 4)
            {
                throw new ArgumentException("验证码至少需要 4 个有效字符。", nameof(options));
            }

            Guid docId = Guid.NewGuid();
            byte[] fileSalt = CodeAndCryptoService.GenerateRandomBytes(SaltSize);
            byte[] docKey = CodeAndCryptoService.GenerateRandomBytes(DocKeySize);
            byte[] keyWrapNonce = CodeAndCryptoService.GenerateRandomBytes(NonceSize);
            byte[] payloadNonce = CodeAndCryptoService.GenerateRandomBytes(NonceSize);
            byte[]? kek = null;
            byte[]? policyPlaintext = null;

            string fullOutputPath = Path.GetFullPath(outputSpdfPath);
            string tempPath = fullOutputPath + "." + Guid.NewGuid().ToString("N").Substring(0, 8) + ".tmp";

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

                string? dir = Path.GetDirectoryName(fullOutputPath);
                if (!string.IsNullOrEmpty(dir)) Directory.CreateDirectory(dir);

                // 先写临时文件再改名：中途失败（磁盘满 / 被占用）不会留下半截的 .spdf，也不会破坏已有的同名文件
                using (var fs = new FileStream(tempPath, FileMode.CreateNew, FileAccess.Write, FileShare.None, 81920))
                using (var writer = new BinaryWriter(fs, Encoding.UTF8, leaveOpen: false))
                {
                    writer.Write(aad);                       // magic | version | docId | salt | N | r | p
                    writer.Write(keyWrapNonce);
                    writer.Write(wrappedPolicy.Length);
                    writer.Write(wrappedPolicy);
                    writer.Write(payloadNonce);
                    writer.Write(encryptedPayload.LongLength);
                    writer.Write(encryptedPayload);
                }

                if (File.Exists(fullOutputPath)) File.Delete(fullOutputPath);
                File.Move(tempPath, fullOutputPath);

                return (docId, verificationCode);
            }
            finally
            {
                CodeAndCryptoService.ZeroMemory(docKey);
                CodeAndCryptoService.ZeroMemory(kek);
                CodeAndCryptoService.ZeroMemory(policyPlaintext);
                try
                {
                    if (File.Exists(tempPath)) File.Delete(tempPath);
                }
                catch (IOException) { }
                catch (UnauthorizedAccessException) { }
            }
        }

        public static SpdfEnvelopeInfo ReadEnvelope(string spdfPath)
        {
            using (var fs = new FileStream(spdfPath, FileMode.Open, FileAccess.Read, FileShare.Read))
            using (var reader = new BinaryReader(fs, Encoding.UTF8, leaveOpen: false))
            {
                Envelope env = ReadEnvelopeHeader(reader);
                return new SpdfEnvelopeInfo
                {
                    DocId = env.DocId,
                    FormatVersion = env.Version,
                    FileSizeBytes = fs.Length
                };
            }
        }

        public static UnlockedSpdfDocument Unlock(string spdfPath, string verificationCode)
        {
            string normalizedCode = CodeAndCryptoService.NormalizeCode(verificationCode);
            if (string.IsNullOrEmpty(normalizedCode))
            {
                throw new CryptographicException("请输入验证码。");
            }

            Envelope env;
            byte[] keyWrapNonce;
            byte[] wrappedPolicy;
            byte[] payloadNonce;
            byte[] encryptedPayload;

            using (var fs = new FileStream(spdfPath, FileMode.Open, FileAccess.Read, FileShare.Read, 81920))
            using (var reader = new BinaryReader(fs, Encoding.UTF8, leaveOpen: false))
            {
                try
                {
                    env = ReadEnvelopeHeader(reader);

                    // 下限沿用旧版（防止被换成过弱的参数），上限同时约束内存占用（≤128 MB）
                    if (env.KdfCostN < 1024 ||
                        !CodeAndCryptoService.IsAcceptableScryptCost(env.KdfCostN, env.KdfBlockSizeR, env.KdfParallelP))
                    {
                        throw new InvalidDataException("文件头中的密钥派生参数无效，文件可能已损坏。");
                    }

                    keyWrapNonce = ReadExact(reader, NonceSize);
                    int wrappedLen = reader.ReadInt32();
                    if (wrappedLen <= TagSize || wrappedLen > MaxPolicyBlockBytes)
                    {
                        throw new InvalidDataException("策略数据块长度无效，文件可能已损坏。");
                    }
                    wrappedPolicy = ReadExact(reader, wrappedLen);

                    payloadNonce = ReadExact(reader, NonceSize);
                    long payloadLen = reader.ReadInt64();
                    if (payloadLen <= TagSize || payloadLen > fs.Length - fs.Position)
                    {
                        throw new InvalidDataException("文件不完整（可能未下载 / 传输完整），请重新获取。");
                    }
                    if (payloadLen > MaxPdfBytes + TagSize)
                    {
                        throw new InvalidDataException("文档体积超过 1 GB，无法打开。");
                    }
                    encryptedPayload = ReadExact(reader, (int)payloadLen);
                }
                catch (EndOfStreamException)
                {
                    throw new InvalidDataException("文件不完整（可能未下载 / 传输完整），请重新获取。");
                }
            }

            byte[] aad = BuildEnvelopeAad(env.DocId, env.Version, env.Salt, env.KdfCostN, env.KdfBlockSizeR, env.KdfParallelP);
            byte[]? kek = null;
            byte[]? policyPlaintext = null;
            byte[]? docKey = null;

            try
            {
                kek = CodeAndCryptoService.DeriveKeyScrypt(normalizedCode, env.Salt, env.KdfCostN, env.KdfBlockSizeR, env.KdfParallelP);

                try
                {
                    policyPlaintext = CodeAndCryptoService.DecryptAesGcm(kek, keyWrapNonce, wrappedPolicy, aad);
                }
                catch (CryptographicException ex)
                {
                    throw new CryptographicException("验证码不正确（或文件头已被改动）。请核对后重新输入。", ex);
                }

                SpdfPolicy policy;
                (docKey, policy) = DeserializePolicyAndKey(env.DocId, policyPlaintext);

                byte[] pdfBytes;
                try
                {
                    pdfBytes = CodeAndCryptoService.DecryptAesGcm(docKey, payloadNonce, encryptedPayload, aad);
                }
                catch (CryptographicException ex)
                {
                    throw new CryptographicException("验证码正确，但文档内容已损坏或被篡改，无法解密。请重新获取文件。", ex);
                }

                return new UnlockedSpdfDocument(policy, pdfBytes);
            }
            finally
            {
                CodeAndCryptoService.ZeroMemory(kek);
                CodeAndCryptoService.ZeroMemory(policyPlaintext);
                CodeAndCryptoService.ZeroMemory(docKey);
            }
        }

        private sealed class Envelope
        {
            public ushort Version;
            public Guid DocId;
            public byte[] Salt = Array.Empty<byte>();
            public int KdfCostN;
            public int KdfBlockSizeR;
            public int KdfParallelP;
        }

        private static Envelope ReadEnvelopeHeader(BinaryReader reader)
        {
            try
            {
                byte[] magic = reader.ReadBytes(4);
                if (magic.Length != 4 ||
                    magic[0] != MagicBytes[0] || magic[1] != MagicBytes[1] ||
                    magic[2] != MagicBytes[2] || magic[3] != MagicBytes[3])
                {
                    throw new InvalidDataException("不是有效的 .spdf 文件。");
                }

                ushort version = reader.ReadUInt16();
                if (version != CurrentVersion)
                {
                    throw new NotSupportedException($"不支持的 .spdf 格式版本 ({version})，请升级阅读端。");
                }

                return new Envelope
                {
                    Version = version,
                    DocId = new Guid(ReadExact(reader, 16)),
                    Salt = ReadExact(reader, SaltSize),
                    KdfCostN = reader.ReadInt32(),
                    KdfBlockSizeR = reader.ReadInt32(),
                    KdfParallelP = reader.ReadInt32()
                };
            }
            catch (EndOfStreamException)
            {
                throw new InvalidDataException("文件不完整（可能未下载 / 传输完整），请重新获取。");
            }
        }

        private static byte[] ReadExact(BinaryReader reader, int count)
        {
            byte[] data = reader.ReadBytes(count);
            if (data.Length != count) throw new EndOfStreamException();
            return data;
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
                w.Write(options.ExpiresAtUtc.HasValue ? options.ExpiresAtUtc.Value.UtcTicks : 0L);
                w.Write((byte)options.MoireLevel);
                w.Write((uint)options.SecurityFlags);
                w.Write(options.OriginalFileName ?? string.Empty);
                w.Write(options.WatermarkNote ?? string.Empty);
                w.Flush();

                byte[] result = ms.ToArray();
                // MemoryStream 的内部缓冲里还留有一份文档密钥，一并擦除
                Array.Clear(ms.GetBuffer(), 0, (int)ms.Length);
                return result;
            }
        }

        private static (byte[] DocKey, SpdfPolicy Policy) DeserializePolicyAndKey(Guid docId, byte[] plaintext)
        {
            try
            {
                using (var ms = new MemoryStream(plaintext, writable: false))
                using (var r = new BinaryReader(ms, Encoding.UTF8))
                {
                    byte innerVersion = r.ReadByte();
                    if (innerVersion != 1)
                    {
                        throw new NotSupportedException($"不支持的策略版本 ({innerVersion})，请升级阅读端。");
                    }

                    byte[] docKey = ReadExact(r, DocKeySize);
                    int maxPrints = r.ReadInt32();
                    long expiresTicks = r.ReadInt64();
                    byte moireRaw = r.ReadByte();
                    SpdfSecurityFlags flags = (SpdfSecurityFlags)r.ReadUInt32();
                    string fileName = r.ReadString();
                    string note = r.ReadString();

                    DateTimeOffset? expires = null;
                    if (expiresTicks > 0)
                    {
                        expires = new DateTimeOffset(Math.Min(expiresTicks, DateTimeOffset.MaxValue.Ticks), TimeSpan.Zero);
                    }

                    var policy = new SpdfPolicy
                    {
                        DocId = docId,
                        MaxPrints = Math.Max(0, maxPrints),
                        ExpiresAtUtc = expires,
                        MoireLevel = moireRaw <= (byte)MoireStrength.High ? (MoireStrength)moireRaw : MoireStrength.Medium,
                        SecurityFlags = flags,
                        OriginalFileName = fileName,
                        WatermarkNote = note
                    };

                    return (docKey, policy);
                }
            }
            catch (EndOfStreamException)
            {
                throw new InvalidDataException("文档策略数据不完整，文件可能已损坏。");
            }
        }
    }
}
```

## `src/SecurePdf.Core/Licensing/DeviceAndNetworkContext.cs`

**模块说明**：采集 MachineGuid、BIOS UUID（含 3 次 WMI 重试）、系统卷序列号与 OS 构建号生成设备指纹，并获取活跃主网卡 IPv4 及本机地址集合（共 `197` 行）

```csharp
using System;
using System.Collections.Generic;
using System.Linq;
using System.Management;
using System.Net.NetworkInformation;
using System.Net.Sockets;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using Microsoft.Win32;
using SecurePdf.Core.Crypto;

namespace SecurePdf.Core.Licensing
{
    public sealed class RuntimeTraceContext
    {
        public string MachineCode { get; set; } = string.Empty;
        public byte[] MachineHash { get; set; } = Array.Empty<byte>();
        public string LocalIpAddress { get; set; } = "127.0.0.1";
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
                LocalIpAddress = GetPrimaryLocalIpAddress()
            };
        }

        /// <summary>
        /// 机器指纹 = SHA-256(MachineGuid | BIOS UUID | 系统卷序列号 | 计算机名)。
        /// 该值同时是本地打印配额记录的 HMAC 密钥，算法与字段顺序不可改动，否则已有配额记录会全部失效。
        /// </summary>
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
                if (i > 0 && i % 4 == 0) sb.Append('-');
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
            catch (NetworkInformationException) { }
            catch (InvalidOperationException) { }

            return "127.0.0.1";
        }

        /// <summary>本机全部单播地址（供打印守卫识别“指向本机的网络端口”）。</summary>
        public static HashSet<string> GetAllLocalAddresses()
        {
            var set = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            try
            {
                foreach (NetworkInterface ni in NetworkInterface.GetAllNetworkInterfaces())
                {
                    foreach (UnicastIPAddressInformation ua in ni.GetIPProperties().UnicastAddresses)
                    {
                        set.Add(ua.Address.ToString());
                    }
                }
            }
            catch (NetworkInformationException) { }
            catch (InvalidOperationException) { }
            return set;
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
            catch (Exception ex) when (ex is System.Security.SecurityException || ex is UnauthorizedAccessException || ex is System.IO.IOException) { }
            return string.Empty;
        }

        private static string ReadBiosUuid()
        {
            // WMI 在系统刚启动时偶尔会瞬时失败；失败一次就会让机器指纹变化、配额记录被判为损坏，所以这里重试
            for (int attempt = 0; attempt < 3; attempt++)
            {
                try
                {
                    using (var searcher = new ManagementObjectSearcher("SELECT UUID FROM Win32_ComputerSystemProduct"))
                    using (ManagementObjectCollection results = searcher.Get())
                    {
                        foreach (ManagementBaseObject obj in results)
                        {
                            using (obj)
                            {
                                string? uuid = obj["UUID"]?.ToString();
                                if (!string.IsNullOrWhiteSpace(uuid)) return uuid!;
                            }
                        }
                    }
                    return string.Empty;
                }
                catch (Exception ex) when (ex is ManagementException || ex is COMException || ex is UnauthorizedAccessException)
                {
                    Thread.Sleep(150);
                }
            }
            return string.Empty;
        }

        private static string ReadSystemVolumeSerial()
        {
            string root = Environment.GetFolderPath(Environment.SpecialFolder.Windows);
            string driveRoot = string.IsNullOrEmpty(root) ? "C:\\" : System.IO.Path.GetPathRoot(root) ?? "C:\\";
            if (GetVolumeInformation(driveRoot, null, 0, out uint serial, out _, out _, null, 0))
            {
                return serial.ToString("X8");
            }
            return string.Empty;
        }
    }
}
```

## `src/SecurePdf.Core/Storage/LocalQuotaStore.cs`

**模块说明**：打印次数与小时级有效期的 DPAPI (`CurrentUser`) + HMAC-SHA256 文件/注册表双存储状态机，含 `Local\` 命名互斥锁、基于 `expiresAtUtc` 条件的 5 分钟容差时钟回拨锁定及双边异常 fail-closed 处理（共 `441` 行）

```csharp
using System;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
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

    /// <summary>本地授权状态无法读写（磁盘 / 注册表故障或被设为只读）。此时一律拒绝访问，不能放行。</summary>
    public sealed class QuotaStoreException : Exception
    {
        public QuotaStoreException(string message, Exception innerException) : base(message, innerException) { }
    }

    public interface IQuotaPersistenceBackend
    {
        byte[]? ReadPrimary(Guid docId);
        byte[]? ReadSecondary(Guid docId);
        void WritePrimary(Guid docId, byte[] protectedBytes);
        void WriteSecondary(Guid docId, byte[] protectedBytes);
    }

    /// <summary>主存储：%LOCALAPPDATA%\SecurePdfSuite\Quotas\{docId}.dat；副存储：HKCU\Software\SecurePdfSuite\Quotas。</summary>
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

        private string FileFor(Guid docId) => Path.Combine(_directoryPath, docId.ToString("N") + ".dat");

        public byte[]? ReadPrimary(Guid docId)
        {
            string file = FileFor(docId);
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
            string file = FileFor(docId);
            string temp = file + ".tmp";

            // 先写临时文件再替换，断电 / 崩溃时不会留下写了一半的记录
            File.WriteAllBytes(temp, protectedBytes);
            if (File.Exists(file))
            {
                try
                {
                    File.Replace(temp, file, null);
                    return;
                }
                catch (IOException)
                {
                    File.Delete(file);
                }
            }
            File.Move(temp, file);
        }

        public void WriteSecondary(Guid docId, byte[] protectedBytes)
        {
            using (RegistryKey key = Registry.CurrentUser.CreateSubKey(_registryBasePath))
            {
                key.SetValue(docId.ToString("N"), protectedBytes, RegistryValueKind.Binary);
            }
        }
    }

    /// <summary>
    /// 打印次数 / 有效期的本地状态机。每条记录 = HMAC-SHA256(机器指纹) 签名后再经 DPAPI(当前用户) 加密，
    /// 同时写入文件与注册表两处；任意一处被删 / 被改都能从另一处恢复并取“已用次数较大者”。
    /// </summary>
    public sealed class LocalQuotaStore
    {
        private static readonly TimeSpan ClockRollbackTolerance = TimeSpan.FromMinutes(5);
        private static readonly byte[] AppEntropy = Encoding.UTF8.GetBytes("SecurePdfSuite-Quota-v2");
        private const int MacLength = 32;
        private const int PayloadLength = 1 + 16 + 4 + 4 + 8 + 8;

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
            return WithDocumentLock(docId, () => LoadAndVerifyCore(docId, policyMaxPrints, expiresAtUtc, currentUtcNow ?? DateTimeOffset.UtcNow));
        }

        /// <summary>占用一次打印额度。额度不足 / 已过期 / 状态异常时抛出 <see cref="InvalidOperationException"/>。</summary>
        public QuotaSnapshot ReservePrintSlot(
            Guid docId,
            int policyMaxPrints,
            DateTimeOffset? expiresAtUtc,
            DateTimeOffset? currentUtcNow = null)
        {
            DateTimeOffset now = currentUtcNow ?? DateTimeOffset.UtcNow;
            return WithDocumentLock(docId, () =>
            {
                QuotaAccessResult status = LoadAndVerifyCore(docId, policyMaxPrints, expiresAtUtc, now);
                if (!status.IsAllowed)
                {
                    throw new InvalidOperationException(status.BlockReason);
                }
                if (status.Snapshot.RemainingPrints <= 0)
                {
                    throw new InvalidOperationException("已达到最大允许打印次数，禁止继续打印。");
                }

                var updated = new QuotaSnapshot
                {
                    DocId = docId,
                    MaxPrints = status.Snapshot.MaxPrints,
                    UsedPrints = status.Snapshot.UsedPrints + 1,
                    SequenceNumber = status.Snapshot.SequenceNumber + 1,
                    MaxSeenUtcTicks = Math.Max(status.Snapshot.MaxSeenUtcTicks, now.UtcTicks)
                };

                SaveBoth(updated);
                return updated;
            });
        }

        /// <summary>归还一次尚未真正送出任何页面的打印额度。以磁盘上的最新状态为准，而不是调用方手里的旧快照。</summary>
        public QuotaSnapshot RollbackPrintSlot(QuotaSnapshot reservedSnapshot)
        {
            if (reservedSnapshot == null) throw new ArgumentNullException(nameof(reservedSnapshot));

            return WithDocumentLock(reservedSnapshot.DocId, () =>
            {
                QuotaSnapshot? primary = TryDecode(ReadBackend(true, reservedSnapshot.DocId), reservedSnapshot.DocId, out _);
                QuotaSnapshot? secondary = TryDecode(ReadBackend(false, reservedSnapshot.DocId), reservedSnapshot.DocId, out _);
                QuotaSnapshot current = Merge(primary, secondary) ?? reservedSnapshot;

                var rolledBack = new QuotaSnapshot
                {
                    DocId = reservedSnapshot.DocId,
                    MaxPrints = current.MaxPrints,
                    UsedPrints = Math.Max(0, current.UsedPrints - 1),
                    SequenceNumber = current.SequenceNumber + 1,
                    MaxSeenUtcTicks = current.MaxSeenUtcTicks
                };

                SaveBoth(rolledBack);
                return rolledBack;
            });
        }

        private QuotaAccessResult LoadAndVerifyCore(Guid docId, int policyMaxPrints, DateTimeOffset? expiresAtUtc, DateTimeOffset now)
        {
            QuotaSnapshot? primary = TryDecode(ReadBackend(true, docId), docId, out bool primaryCorrupt);
            QuotaSnapshot? secondary = TryDecode(ReadBackend(false, docId), docId, out bool secondaryCorrupt);

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
                        BlockReason = "本机的文档授权记录无法通过校验（记录被改动，或电脑名称 / 硬件 / Windows 账户已变更），文档已锁定。如需继续使用，请联系发送方重新签发。",
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
            else
            {
                if (primary == null || secondary == null ||
                    primary.UsedPrints != secondary.UsedPrints ||
                    primary.SequenceNumber != secondary.SequenceNumber)
                {
                    tamperDetected = true;
                }

                QuotaSnapshot merged = Merge(primary, secondary)!;
                effective = new QuotaSnapshot
                {
                    DocId = docId,
                    MaxPrints = Math.Min(policyMaxPrints, merged.MaxPrints),
                    UsedPrints = merged.UsedPrints,
                    SequenceNumber = merged.SequenceNumber + 1,
                    MaxSeenUtcTicks = merged.MaxSeenUtcTicks
                };
            }

            // 时间回拨只对“有有效期”的文档有意义；永久有效的文档不因时钟异常（双系统、主板电池没电）被误锁
            bool clockRolledBack = now.UtcTicks + ClockRollbackTolerance.Ticks < effective.MaxSeenUtcTicks;
            if (clockRolledBack && expiresAtUtc.HasValue)
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
                TamperWarning = tamperDetected || clockRolledBack,
                Snapshot = effective
            };
        }

        /// <summary>两份记录取最不利于绕过的组合：已用次数取大、上限取小。</summary>
        private static QuotaSnapshot? Merge(QuotaSnapshot? a, QuotaSnapshot? b)
        {
            if (a == null) return b;
            if (b == null) return a;
            return new QuotaSnapshot
            {
                DocId = a.DocId,
                MaxPrints = Math.Min(a.MaxPrints, b.MaxPrints),
                UsedPrints = Math.Max(a.UsedPrints, b.UsedPrints),
                SequenceNumber = Math.Max(a.SequenceNumber, b.SequenceNumber),
                MaxSeenUtcTicks = Math.Max(a.MaxSeenUtcTicks, b.MaxSeenUtcTicks)
            };
        }

        /// <summary>同一文档的“读取-修改-写回”在本机所有阅读器进程之间串行化，避免同时打开两个窗口各打一次。</summary>
        private static T WithDocumentLock<T>(Guid docId, Func<T> action)
        {
            using (var mutex = new Mutex(false, @"Local\SecurePdfSuite.Quota." + docId.ToString("N")))
            {
                bool owned = false;
                try
                {
                    try
                    {
                        owned = mutex.WaitOne(TimeSpan.FromSeconds(10));
                    }
                    catch (AbandonedMutexException)
                    {
                        owned = true;   // 上一个持有者异常退出，锁已转交给当前线程
                    }

                    if (!owned)
                    {
                        throw new QuotaStoreException("本地授权状态正被另一个阅读器窗口占用，请稍后重试。", new TimeoutException());
                    }
                    return action();
                }
                finally
                {
                    if (owned) mutex.ReleaseMutex();
                }
            }
        }

        private byte[]? ReadBackend(bool primary, Guid docId)
        {
            try
            {
                return primary ? _backend.ReadPrimary(docId) : _backend.ReadSecondary(docId);
            }
            catch (Exception ex) when (IsStorageFailure(ex))
            {
                throw new QuotaStoreException("无法读取本地授权状态，已拒绝访问：" + ex.Message, ex);
            }
        }

        private void SaveBoth(QuotaSnapshot snapshot)
        {
            byte[] encoded = Encode(snapshot);
            try
            {
                _backend.WritePrimary(snapshot.DocId, encoded);
                _backend.WriteSecondary(snapshot.DocId, encoded);
            }
            catch (Exception ex) when (IsStorageFailure(ex))
            {
                throw new QuotaStoreException("无法写入本地授权状态（磁盘已满或没有写入权限），已拒绝访问：" + ex.Message, ex);
            }
        }

        private static bool IsStorageFailure(Exception ex)
        {
            return ex is IOException || ex is UnauthorizedAccessException || ex is System.Security.SecurityException;
        }

        private byte[] Encode(QuotaSnapshot snapshot)
        {
            byte[] payload;
            using (var ms = new MemoryStream(PayloadLength))
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
            if (blob == null || blob.Length == 0) return null;

            // 任何解码失败（DPAPI 解不开、长度不对、签名不符、文档 ID 不符）都按“记录被破坏”处理
            isCorrupt = true;
            try
            {
                byte[] combined = _useDpapi
                    ? ProtectedData.Unprotect(blob, AppEntropy, DataProtectionScope.CurrentUser)
                    : blob;

                if (combined.Length != PayloadLength + MacLength) return null;

                byte[] payload = new byte[PayloadLength];
                byte[] actualMac = new byte[MacLength];
                Buffer.BlockCopy(combined, 0, payload, 0, PayloadLength);
                Buffer.BlockCopy(combined, PayloadLength, actualMac, 0, MacLength);

                byte[] expectedMac = CodeAndCryptoService.ComputeHmacSha256(_machineHash, payload);
                if (!CodeAndCryptoService.FixedTimeEquals(actualMac, expectedMac)) return null;

                using (var ms = new MemoryStream(payload))
                using (var r = new BinaryReader(ms))
                {
                    if (r.ReadByte() != 1) return null;
                    if (new Guid(r.ReadBytes(16)) != expectedDocId) return null;

                    var snapshot = new QuotaSnapshot
                    {
                        DocId = expectedDocId,
                        MaxPrints = r.ReadInt32(),
                        UsedPrints = r.ReadInt32(),
                        SequenceNumber = r.ReadInt64(),
                        MaxSeenUtcTicks = r.ReadInt64()
                    };
                    if (snapshot.MaxPrints < 0 || snapshot.UsedPrints < 0) return null;

                    isCorrupt = false;
                    return snapshot;
                }
            }
            catch (CryptographicException)
            {
                return null;
            }
        }
    }
}
```

---

[返回总览 README](../README.md)
