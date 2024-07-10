defmodule Heimdall.DNS.Model.Question do
  alias Heimdall.DNS.Model

  defstruct qname: nil, qtype: nil, qclass: nil

  def parse(questions, data, 0), do: [questions, data]

  def parse(questions, data, count) do
    [labels, data] = Model.parse_labels([], data)
    <<qtype::16, data::bitstring>> = data
    <<qclass::16, data::bitstring>> = data

    parse(
      [
        %__MODULE__{
          qname: Enum.join(labels, "."),
          qtype: Model.qtype(qtype),
          qclass: Model.qclass(qclass)
        }
        | questions
      ],
      data,
      count - 1
    )
  end
end
