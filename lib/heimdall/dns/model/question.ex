defmodule Heimdall.DNS.Model.Question do
  alias Heimdall.DNS.Decoder
  alias Heimdall.DNS.Encoder

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
