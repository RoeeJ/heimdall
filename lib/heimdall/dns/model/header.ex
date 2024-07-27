defmodule Heimdall.DNS.Model.Header do
  @moduledoc """
  Header for DNS.
  """

  @type t() :: %__MODULE__{
          id: non_neg_integer(),
          qr: boolean(),
          opcode: non_neg_integer(),
          authoritative: boolean(),
          truncated: boolean(),
          recurs_desired: boolean(),
          recurs_available: boolean(),
          z: non_neg_integer(),
          resp_code: non_neg_integer()
        }
  defstruct id: nil,
            qr: nil,
            opcode: nil,
            authoritative: nil,
            truncated: nil,
            recurs_desired: nil,
            recurs_available: nil,
            z: nil,
            resp_code: nil
end
