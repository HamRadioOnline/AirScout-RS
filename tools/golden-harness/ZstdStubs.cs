// Stubs so the binCraft parsers compile without the native zstd library. The golden cases
// only feed uncompressed buffers; decompression is tested on the Rust side.
using System;

namespace ZstdNet
{
    public class Decompressor : IDisposable
    {
        public byte[] Unwrap(byte[] data) => throw new NotSupportedException("zstd stub");
        public void Dispose() { }
    }
}

public static class NativeLoader
{
    public static void LoadLibzstd() { }
}

namespace AirScout.PlaneFeeds.Plugin.PublicBinCraft
{
    public static class NativeLoader
    {
        public static void LoadLibzstd() { }
    }
}
