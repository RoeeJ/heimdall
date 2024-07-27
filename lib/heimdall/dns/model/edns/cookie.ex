defmodule Heimdall.DNS.Model.EDNS.Cookie do
  @moduledoc """
  Represents an EDNS Cookie option.
  """

  @type t() :: %__MODULE__{
    opt_length: non_neg_integer(), client_cookie: bitstring(), server_cookie: bitstring()
  }

  require Logger
  defstruct opt_length: nil, client_cookie: nil, server_cookie: nil

  @spec parse(data :: bitstring()) :: t()
  def parse(data) do
    <<opt_length::16, data::bitstring>> = data
    <<client_cookie::binary-size(8), data::bitstring>> = data

    server_cookie =
      case opt_length - 8 do
        0 ->
          <<>>

        n ->
          <<server_cookie::binary-size(n), _data::bitstring>> = data
          server_cookie
      end

    %__MODULE__{
      opt_length: opt_length,
      client_cookie: client_cookie,
      server_cookie: server_cookie
    }
  end

  @spec encode(cookie :: t()) :: bitstring()
  def encode(%__MODULE__{} = cookie) do
    <<cookie.client_cookie::bitstring, cookie.server_cookie::bitstring>>
  end
end
