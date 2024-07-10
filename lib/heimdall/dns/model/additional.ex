defmodule Heimdall.DNS.Model.Additional do
  alias Heimdall.DNS.Model
  defstruct qtype: nil, qclass: nil, ttl: nil, rdlength: nil, rdata: nil
  def parse(additional, data, 0), do: [additional, data]

  def parse(additional, data, count) do
    [labels, data] = Model.parse_labels([], data)
    <<qtype::16, data::bitstring>> = data

    case Model.qtype(qtype) do
      :opt ->
        [rr, data] = Model.EDNS.parse(data)
        parse([rr | additional], data, count - 1)

      _ ->
        <<qtype::16, qclass::16, ttl::32, rdlength::16, data::bitstring>> = data
        <<rdata::size(rdlength), data::bitstring>> = data

        parse(
          [
            %__MODULE__{
              qtype: Model.qtype(qtype),
              qclass: Model.qclass(qclass),
              ttl: ttl,
              rdlength: rdlength,
              rdata: rdata
            }
            | additional
          ],
          data,
          count - 1
        )
    end
  end
end
