defmodule Heimdall.DNS.Model.Question do
  alias Heimdall.DNS.{Encoder, Decoder, Model}

  @type t() :: %__MODULE__{
          qname: String.t(),
          qclass: Model.qclass_atoms(),
          qtype: Model.qtype_atoms()
        }

  defstruct qname: nil, qtype: nil, qclass: nil

  def parse(questions, data, 0), do: [questions, data]

  def parse(questions, data, count) do
    [labels, data] = Decoder.labels([], data)
    <<qtype::16, data::bitstring>> = data
    <<qclass::16, data::bitstring>> = data

    parse(
      [
        %__MODULE__{
          qname: Enum.join(labels, "."),
          qtype: Decoder.qtype(qtype),
          qclass: Decoder.qclass(qclass)
        }
        | questions
      ],
      data,
      count - 1
    )
  end

  def encode(%__MODULE__{} = question) do
    Encoder.labels(question.qname) <>
      <<Encoder.qtype(question.qtype)::16, Encoder.qclass(question.qclass)::16>>
  end
end
