defmodule Heimdall.DNS.Model.Header do
  alias Heimdall.DNS.Decoder

  @type t() :: %__MODULE__{
          id: non_neg_integer(),
          qr: Model.qr_atoms(),
          opcode: Model.opcode_atoms(),
          authoritative: boolean(),
          truncated: boolean(),
          recurs_desired: boolean(),
          recurs_available: boolean(),
          z: non_neg_integer(),
          resp_code: Model.rcode_atoms(),
          ancount: non_neg_integer(),
          qdcount: non_neg_integer(),
          nscount: non_neg_integer(),
          arcount: non_neg_integer()
        }
  defstruct id: nil,
            qr: nil,
            opcode: nil,
            authoritative: nil,
            truncated: nil,
            recurs_desired: nil,
            recurs_available: nil,
            z: nil,
            resp_code: nil,
            qdcount: nil,
            ancount: nil,
            nscount: nil,
            arcount: nil

  @spec encode(header :: t()) :: bitstring()
  def encode(%__MODULE__{} = header) do
  end

  @spec decode(data :: bitstring()) :: {header :: t(), rest :: bitstring()}
  def decode(
        <<id::16, flags::16, qdcount::16, ancount::16, nscount::16, arcount::16, rest::binary>> =
          data
      ) do
    <<qr::1, opcode::4, aa::1, tc::1, rd::1, ra::1, _z::3, rcode::4>> = <<flags::16>>

    header = %__MODULE__{
      id: id,
      qr: Decoder.qr(qr),
      opcode: Decoder.opcode(opcode),
      authoritative: aa == 1,
      truncated: tc == 1,
      recurs_desired: rd == 1,
      recurs_available: ra == 1,
      resp_code: Decoder.rcode(rcode),
      qdcount: qdcount,
      ancount: ancount,
      nscount: nscount,
      arcount: arcount
    }

    {header, rest}
  end
end
