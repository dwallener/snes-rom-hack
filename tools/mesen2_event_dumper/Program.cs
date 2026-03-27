using System.Runtime.InteropServices;
using System.Text.Json;

if (args.Length < 1)
{
    Console.Error.WriteLine("usage: dotnet run --project tools/mesen2_event_dumper -- <path-to-mesen-debug-dll>");
    return 2;
}

var dllPath = Path.GetFullPath(args[0]);
if (!File.Exists(dllPath))
{
    Console.Error.WriteLine($"error: DLL not found: {dllPath}");
    return 1;
}

Console.WriteLine("Mesen2 event dumper scaffold");
Console.WriteLine($"DLL: {dllPath}");
Console.WriteLine("This tool mirrors the Mesen2 debug-event API but requires an in-process host.");
Console.WriteLine("It will only produce useful output when loaded alongside a live Mesen2 core/debug runtime.");

using var api = MesenDebugApi.Load(dllPath);

try
{
    api.InitializeDebugger();
    api.SetEventViewerConfigSnes(InteropSnesEventViewerConfig.CreateVisibleAll());
    var events = api.GetDebugEvents(CpuType.Snes);

    foreach (var evt in events)
    {
        var row = new
        {
            type = evt.Type.ToString(),
            pc = $"0x{evt.ProgramCounter:X6}",
            scanline = evt.Scanline,
            cycle = evt.Cycle,
            op_addr = $"0x{evt.Operation.Address:X6}",
            op_value = evt.Operation.Value,
            op_type = evt.Operation.Type.ToString(),
            op_mem = evt.Operation.MemType.ToString(),
            dma_channel = evt.DmaChannel,
            dma = evt.DmaChannel >= 0 ? new
            {
                src_bank = evt.DmaChannelInfo.SrcBank,
                src_address = evt.DmaChannelInfo.SrcAddress,
                dest_address = evt.DmaChannelInfo.DestAddress,
                transfer_size = evt.DmaChannelInfo.TransferSize,
                hdma_table = evt.DmaChannelInfo.HdmaTableAddress,
                transfer_mode = evt.DmaChannelInfo.TransferMode,
                dma_active = evt.DmaChannelInfo.DmaActive,
            } : null
        };

        Console.WriteLine(JsonSerializer.Serialize(row));
    }
}
catch (Exception ex)
{
    Console.Error.WriteLine("The API scaffold loaded, but the debugger runtime is not usable in this process.");
    Console.Error.WriteLine(ex.Message);
    Console.Error.WriteLine("Expected next step: host this against a live Mesen2 core/debug DLL environment.");
    return 1;
}
finally
{
    try
    {
        api.ReleaseDebugger();
    }
    catch
    {
    }
}

return 0;

internal sealed class MesenDebugApi : IDisposable
{
    private readonly nint _library;

    private readonly InitializeDebuggerDelegate _initializeDebugger;
    private readonly ReleaseDebuggerDelegate _releaseDebugger;
    private readonly GetDebugEventCountDelegate _getDebugEventCount;
    private readonly GetDebugEventsDelegate _getDebugEvents;
    private readonly SetEventViewerConfigSnesDelegate _setEventViewerConfigSnes;

    private MesenDebugApi(
        nint library,
        InitializeDebuggerDelegate initializeDebugger,
        ReleaseDebuggerDelegate releaseDebugger,
        GetDebugEventCountDelegate getDebugEventCount,
        GetDebugEventsDelegate getDebugEvents,
        SetEventViewerConfigSnesDelegate setEventViewerConfigSnes)
    {
        _library = library;
        _initializeDebugger = initializeDebugger;
        _releaseDebugger = releaseDebugger;
        _getDebugEventCount = getDebugEventCount;
        _getDebugEvents = getDebugEvents;
        _setEventViewerConfigSnes = setEventViewerConfigSnes;
    }

    public static MesenDebugApi Load(string dllPath)
    {
        var library = NativeLibrary.Load(dllPath);
        return new MesenDebugApi(
            library,
            Bind<InitializeDebuggerDelegate>(library, "InitializeDebugger"),
            Bind<ReleaseDebuggerDelegate>(library, "ReleaseDebugger"),
            Bind<GetDebugEventCountDelegate>(library, "GetDebugEventCount"),
            Bind<GetDebugEventsDelegate>(library, "GetDebugEvents"),
            Bind<SetEventViewerConfigSnesDelegate>(library, "SetEventViewerConfig"));
    }

    public void InitializeDebugger() => _initializeDebugger();
    public void ReleaseDebugger() => _releaseDebugger();
    public void SetEventViewerConfigSnes(InteropSnesEventViewerConfig config) => _setEventViewerConfigSnes(CpuType.Snes, config);

    public DebugEventInfo[] GetDebugEvents(CpuType cpuType)
    {
        uint maxCount = _getDebugEventCount(cpuType);
        if (maxCount == 0)
        {
            return Array.Empty<DebugEventInfo>();
        }

        var events = new DebugEventInfo[maxCount];
        _getDebugEvents(cpuType, events, ref maxCount);
        if (events.Length != maxCount)
        {
            Array.Resize(ref events, (int)maxCount);
        }
        return events;
    }

    public void Dispose()
    {
        if (_library != nint.Zero)
        {
            NativeLibrary.Free(_library);
        }
    }

    private static T Bind<T>(nint library, string name) where T : Delegate
    {
        var export = NativeLibrary.GetExport(library, name);
        return Marshal.GetDelegateForFunctionPointer<T>(export);
    }

    [UnmanagedFunctionPointer(CallingConvention.StdCall)]
    private delegate void InitializeDebuggerDelegate();

    [UnmanagedFunctionPointer(CallingConvention.StdCall)]
    private delegate void ReleaseDebuggerDelegate();

    [UnmanagedFunctionPointer(CallingConvention.StdCall)]
    private delegate uint GetDebugEventCountDelegate(CpuType cpuType);

    [UnmanagedFunctionPointer(CallingConvention.StdCall)]
    private delegate void GetDebugEventsDelegate(CpuType cpuType, [In, Out] DebugEventInfo[] eventArray, ref uint maxEventCount);

    [UnmanagedFunctionPointer(CallingConvention.StdCall)]
    private delegate void SetEventViewerConfigSnesDelegate(CpuType cpuType, InteropSnesEventViewerConfig config);
}

internal enum CpuType : byte
{
    Snes = 0,
}

internal enum MemoryOperationType
{
    Read = 0,
    Write = 1,
    ExecOpCode = 2,
    ExecOperand = 3,
    DmaRead = 4,
    DmaWrite = 5,
    DummyRead = 6,
    DummyWrite = 7,
    PpuRenderingRead = 8,
    Idle = 9,
}

internal enum MemoryType
{
    None = 255,
}

internal enum DebugEventType
{
    Register,
    Nmi,
    Irq,
    Breakpoint,
    BgColorChange,
    SpriteZeroHit,
    DmcDmaRead,
    DmaRead,
}

[Flags]
internal enum EventFlags
{
    PreviousFrame = 1 << 0,
    RegFirstWrite = 1 << 1,
    RegSecondWrite = 1 << 2,
    HasTargetMemory = 1 << 3,
    SmsVdpPaletteWrite = 1 << 4,
    ReadWriteOp = 1 << 5,
}

[StructLayout(LayoutKind.Sequential)]
internal struct MemoryOperationInfo
{
    public uint Address;
    public int Value;
    public MemoryOperationType Type;
    public MemoryType MemType;
}

[StructLayout(LayoutKind.Sequential)]
internal struct DmaChannelConfig
{
    public ushort SrcAddress;
    public ushort TransferSize;
    public ushort HdmaTableAddress;
    public byte SrcBank;
    public byte DestAddress;
    [MarshalAs(UnmanagedType.I1)] public bool DmaActive;
    [MarshalAs(UnmanagedType.I1)] public bool InvertDirection;
    [MarshalAs(UnmanagedType.I1)] public bool Decrement;
    [MarshalAs(UnmanagedType.I1)] public bool FixedTransfer;
    [MarshalAs(UnmanagedType.I1)] public bool HdmaIndirectAddressing;
    public byte TransferMode;
    public byte HdmaBank;
    public byte HdmaLineCounterAndRepeat;
    [MarshalAs(UnmanagedType.I1)] public bool DoTransfer;
    [MarshalAs(UnmanagedType.I1)] public bool HdmaFinished;
    [MarshalAs(UnmanagedType.I1)] public bool UnusedControlFlag;
    public byte UnusedRegister;
}

[StructLayout(LayoutKind.Sequential)]
internal struct DebugEventInfo
{
    public MemoryOperationInfo Operation;
    public DebugEventType Type;
    public uint ProgramCounter;
    public short Scanline;
    public ushort Cycle;
    public short BreakpointId;
    public sbyte DmaChannel;
    public DmaChannelConfig DmaChannelInfo;
    public EventFlags Flags;
    public int RegisterId;
    public MemoryOperationInfo TargetMemory;
    public uint Color;
}

[StructLayout(LayoutKind.Sequential)]
internal struct InteropEventViewerCategoryCfg
{
    [MarshalAs(UnmanagedType.I1)] public bool Visible;
    public uint Color;
}

[StructLayout(LayoutKind.Sequential)]
internal sealed class InteropSnesEventViewerConfig
{
    public InteropEventViewerCategoryCfg Irq;
    public InteropEventViewerCategoryCfg Nmi;
    public InteropEventViewerCategoryCfg MarkedBreakpoints;
    public InteropEventViewerCategoryCfg PpuRegisterReads;
    public InteropEventViewerCategoryCfg PpuRegisterCgramWrites;
    public InteropEventViewerCategoryCfg PpuRegisterVramWrites;
    public InteropEventViewerCategoryCfg PpuRegisterOamWrites;
    public InteropEventViewerCategoryCfg PpuRegisterMode7Writes;
    public InteropEventViewerCategoryCfg PpuRegisterBgOptionWrites;
    public InteropEventViewerCategoryCfg PpuRegisterBgScrollWrites;
    public InteropEventViewerCategoryCfg PpuRegisterWindowWrites;
    public InteropEventViewerCategoryCfg PpuRegisterOtherWrites;
    public InteropEventViewerCategoryCfg ApuRegisterReads;
    public InteropEventViewerCategoryCfg ApuRegisterWrites;
    public InteropEventViewerCategoryCfg CpuRegisterReads;
    public InteropEventViewerCategoryCfg CpuRegisterWrites;
    public InteropEventViewerCategoryCfg WorkRamRegisterReads;
    public InteropEventViewerCategoryCfg WorkRamRegisterWrites;
    [MarshalAs(UnmanagedType.I1)] public bool ShowPreviousFrameEvents;
    [MarshalAs(UnmanagedType.ByValArray, SizeConst = 8)] public byte[] ShowDmaChannels = new byte[8];

    public static InteropSnesEventViewerConfig CreateVisibleAll()
    {
        var visible = new InteropEventViewerCategoryCfg { Visible = true, Color = 0xFFFFFFFF };
        return new InteropSnesEventViewerConfig
        {
            Irq = visible,
            Nmi = visible,
            MarkedBreakpoints = visible,
            PpuRegisterReads = visible,
            PpuRegisterCgramWrites = visible,
            PpuRegisterVramWrites = visible,
            PpuRegisterOamWrites = visible,
            PpuRegisterMode7Writes = visible,
            PpuRegisterBgOptionWrites = visible,
            PpuRegisterBgScrollWrites = visible,
            PpuRegisterWindowWrites = visible,
            PpuRegisterOtherWrites = visible,
            ApuRegisterReads = visible,
            ApuRegisterWrites = visible,
            CpuRegisterReads = visible,
            CpuRegisterWrites = visible,
            WorkRamRegisterReads = visible,
            WorkRamRegisterWrites = visible,
            ShowPreviousFrameEvents = false,
            ShowDmaChannels = Enumerable.Repeat((byte)1, 8).ToArray(),
        };
    }
}
