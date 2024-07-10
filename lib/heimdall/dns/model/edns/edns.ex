defmodule Heimdall.DNS.Model.EDNS do
  alias Heimdall.DNS.Model.EDNS

  defstruct payload_size: nil, rcode: nil, version: nil, z: nil, data_length: nil, data: nil

  def parse(data) do
    <<payload_size::16, rcode::8, version::8, z::16, data_length::16, data::bitstring>> = data
    <<edns_data::binary-size(data_length), data::bitstring>> = data
    option = parse_option(edns_data)
    [
      %__MODULE__{
        payload_size: payload_size,
        rcode: rcode,
        version: version,
        z: z,
        data_length: data_length,
        data: option
      },
      data
    ]
  end

  def parse_option(data) do
    <<type::16, data::bitstring>> = data

    case option_type(type) do
      :cookie ->
        EDNS.Cookie.parse(data)
      n -> throw("Unknown OPT value #{n}")
    end
  end

  defp option_type(n) do
    case n do
      10 -> :cookie
      _ -> throw("Unknown OPT type #{n}")
    end
  end
end
