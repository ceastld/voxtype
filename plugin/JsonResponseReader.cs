using System;
using System.Text;

namespace VoxType.Plugin;

internal static class JsonResponseReader
{
    public static string? ReadString(string json, string key)
    {
        if (string.IsNullOrWhiteSpace(json))
        {
            return null;
        }

        var token = $"\"{key}\":";
        var idx = json.IndexOf(token, StringComparison.Ordinal);
        if (idx < 0)
        {
            return null;
        }

        idx += token.Length;
        while (idx < json.Length && char.IsWhiteSpace(json[idx]))
        {
            idx++;
        }

        if (idx >= json.Length)
        {
            return null;
        }

        if (json[idx] == '"')
        {
            idx++;
            var sb = new StringBuilder();
            while (idx < json.Length)
            {
                var ch = json[idx++];
                if (ch == '\\' && idx < json.Length)
                {
                    var next = json[idx++];
                    sb.Append(next switch
                    {
                        '"' => '"',
                        '\\' => '\\',
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        _ => next,
                    });
                    continue;
                }

                if (ch == '"')
                {
                    return sb.ToString();
                }

                sb.Append(ch);
            }

            return sb.ToString();
        }

        return null;
    }
}
