defmodule Heimdall.Util.BitStringComparer do
  def compare(a, b) when is_bitstring(a) and is_bitstring(b) do
    do_compare(a, b, 0)
  end

  defp do_compare(<<>>, <<>>, _index), do: IO.puts("Bitstrings are identical")
  defp do_compare(<<>>, _b, _index), do: IO.puts("Bitstrings are identical up to the length of the shorter one")
  defp do_compare(_a, <<>>, _index), do: IO.puts("Bitstrings are identical up to the length of the shorter one")
  defp do_compare(<<x, a::binary>>, <<x, b::binary>>, index), do: do_compare(a, b, index + 1)
  defp do_compare(<<x, _::binary>>, <<y, _::binary>>, index), do: IO.puts("Difference found at byte index #{index}: #{x} vs #{y}")
end
