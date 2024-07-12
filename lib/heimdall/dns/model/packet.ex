defmodule Heimdall.DNS.Model.Packet do
  alias Heimdall.DNS.Model

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
          arcount: non_neg_integer(),
          questions: [Model.Question.t()],
          answers: [Model.ResourceRecord.t()],
          nameservers: [Model.ResourceRecord.t()],
          additional: [Model.ResourceRecord.t()]
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
            arcount: nil,
            questions: [],
            answers: [],
            nameservers: [],
            additional: []
end
