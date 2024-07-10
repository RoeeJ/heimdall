defmodule Heimdall.DNS.Model.EDNS.Cookie do
  defstruct client_cookie: nil, server_cookie: nil

  def parse(data) do
    <<opt_length::16, data::bitstring>> = data
    <<client_cookie::binary-size(8), data::bitstring>> = data

    [
      %__MODULE__{
        client_cookie: client_cookie,
        server_cookie: nil
      },
      data
    ]
  end
end
