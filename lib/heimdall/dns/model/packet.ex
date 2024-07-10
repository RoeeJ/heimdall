defmodule Heimdall.DNS.Model.Packet do
  @type t() :: %__MODULE__{}
  defstruct id: nil,
            qr: nil,
            opcode: nil,
            aa: nil,
            tc: nil,
            rd: nil,
            ra: nil,
            z: nil,
            rcode: nil,
            qdcount: nil,
            ancount: nil,
            nscount: nil,
            arcount: nil,
            questions: [],
            answers: [],
            nameservers: [],
            additional: []
end
