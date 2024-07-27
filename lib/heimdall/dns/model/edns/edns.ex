defmodule Heimdall.DNS.Model.EDNS do
  @moduledoc """
  EDNS model.
  """
  require Logger

  @type t() :: %__MODULE__{
          version: non_neg_integer(),
          dnssec_ok: boolean(),
          options: [any()]
        }

  defstruct version: nil, dnssec_ok: nil, options: []

  def encode_option({:cookie, <<data::bitstring>>}) do
    <<10::16, byte_size(<<data::bitstring>>)::16, data::bitstring>>
  end
end
