using System;
using System.Diagnostics;
using System.IO;
using System.Net.Http;
using System.Reflection;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

namespace VoxType.Plugin;

internal static class InstallerDownload
{
    private static readonly HttpClient Http = new HttpClient
    {
        Timeout = TimeSpan.FromMinutes(30),
    };

    public static async Task<string> DownloadLatestAsync(CancellationToken cancellationToken = default)
    {
        var channel = LoadChannel();
        var urls = new[] { channel.InstallerMirrorUrl, channel.InstallerUrl };
        Exception? lastError = null;

        foreach (var url in urls)
        {
            if (string.IsNullOrWhiteSpace(url))
            {
                continue;
            }

            try
            {
                return await DownloadFromUrlAsync(url, channel, cancellationToken)
                    .ConfigureAwait(false);
            }
            catch (Exception ex)
            {
                lastError = ex;
            }
        }

        throw lastError ?? new InvalidOperationException("未配置 VoxType 安装包下载地址");
    }

    private static async Task<string> DownloadFromUrlAsync(
        string url,
        VoxTypePluginChannel channel,
        CancellationToken cancellationToken)
    {
        var fileName = Path.GetFileName(new Uri(url).LocalPath);
        if (string.IsNullOrWhiteSpace(fileName))
        {
            fileName = $"VoxType_{channel.ClientVersion}_x64-setup.exe";
        }

        var targetDir = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.UserProfile),
            "Downloads");
        Directory.CreateDirectory(targetDir);
        var targetPath = Path.Combine(targetDir, fileName);

        using var response = await Http.GetAsync(url, HttpCompletionOption.ResponseHeadersRead, cancellationToken)
            .ConfigureAwait(false);
        response.EnsureSuccessStatusCode();

        using (var input = await response.Content.ReadAsStreamAsync().ConfigureAwait(false))
        using (var output = File.Create(targetPath))
        {
            await input.CopyToAsync(output).ConfigureAwait(false);
        }

        if (!string.IsNullOrWhiteSpace(channel.InstallerSha256))
        {
            VerifySha256(targetPath, channel.InstallerSha256!);
        }

        return targetPath;
    }

    public static void LaunchInstaller(string installerPath)
    {
        if (string.IsNullOrWhiteSpace(installerPath) || !File.Exists(installerPath))
        {
            throw new FileNotFoundException("安装包不存在", installerPath);
        }

        Process.Start(new ProcessStartInfo
        {
            FileName = installerPath,
            UseShellExecute = true,
            WorkingDirectory = Path.GetDirectoryName(installerPath) ?? "",
        });
    }

    private static void VerifySha256(string path, string expectedHex)
    {
        using var sha = SHA256.Create();
        using var stream = File.OpenRead(path);
        var hash = sha.ComputeHash(stream);
        var actual = BitConverter.ToString(hash).Replace("-", "").ToLowerInvariant();
        var expected = expectedHex.Trim().ToLowerInvariant();
        if (!actual.Equals(expected, StringComparison.Ordinal))
        {
            File.Delete(path);
            throw new InvalidOperationException("安装包校验失败，已删除下载文件");
        }
    }

    private static VoxTypePluginChannel LoadChannel()
    {
        var path = ResolveChannelPath();
        if (!File.Exists(path))
        {
            return VoxTypePluginChannel.Default();
        }

        var raw = File.ReadAllText(path, Encoding.UTF8);
        return VoxTypePluginChannel.Parse(raw);
    }

    private static string ResolveChannelPath()
    {
        var assemblyDir = Path.GetDirectoryName(Assembly.GetExecutingAssembly().Location);
        if (!string.IsNullOrWhiteSpace(assemblyDir))
        {
            var nextToDll = Path.Combine(assemblyDir, "voxtype-plugin-channel.json");
            if (File.Exists(nextToDll))
            {
                return nextToDll;
            }
        }

        return Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "voxtype-plugin-channel.json");
    }
}

internal sealed class VoxTypePluginChannel
{
    public string ClientVersion { get; set; } = "0.1.7";

    public string InstallerUrl { get; set; } = "";

    public string InstallerMirrorUrl { get; set; } = "";

    public string? InstallerSha256 { get; set; }

    public static VoxTypePluginChannel Default() =>
        new()
        {
            ClientVersion = "0.1.7",
            InstallerUrl =
                "https://github.com/ceastld/voxtype/releases/download/v0.1.7/VoxType_0.1.7_x64-setup.exe",
            InstallerSha256 =
                "e868243c887389056c518bab2f0003a9bb5a609190bd9679e396e083830ec73b",
        };

    public static VoxTypePluginChannel Parse(string raw)
    {
        var channel = Default();
        channel.ClientVersion = ReadString(raw, "clientVersion") ?? channel.ClientVersion;
        channel.InstallerUrl = ReadString(raw, "installerUrl") ?? channel.InstallerUrl;
        channel.InstallerMirrorUrl = ReadString(raw, "installerMirrorUrl") ?? channel.InstallerMirrorUrl;
        channel.InstallerSha256 = ReadString(raw, "installerSha256") ?? channel.InstallerSha256;
        return channel;
    }

    private static string? ReadString(string json, string key) =>
        JsonResponseReader.ReadString(json, key);
}
