defmodule Heimdall.DNS.Model.Additional do
  @type t :: %__MODULE__{}

  require Logger
  alias Heimdall.DNS.Model
  alias Heimdall.DNS.{Encoder, Decoder}

  defstruct qtype: 0, qclass: 0, ttl: 0, rdlength: 0, rdata: nil
  def parse(additional, data, 0), do: [additional, data]

  def parse(additional, data, count) do
    [_, data] = Decoder.labels([], data)
    <<qtype::16, data::bitstring>> = data

    case Decoder.qtype(qtype) do
      :opt ->
        [rr, data] = Model.EDNS.parse(data)

        ar = %Model.Additional{
          qtype: Encoder.qtype(:opt),
          rdata: rr
        }

        parse([ar | additional], data, count - 1)

      _ ->
        <<qtype::16, qclass::16, ttl::32, rdlength::16, data::bitstring>> = data
        <<rdata::size(rdlength), data::bitstring>> = data

        parse(
          [
            %__MODULE__{
              qtype: Decoder.qtype(qtype),
              qclass: Decoder.qclass(qclass),
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

  @spec encode(Model.Additional.t()) :: bitstring()
  def encode(%Model.Additional{} = additional) do
    case additional.rdata do
      %Model.EDNS{} = edns ->
        res =
          <<0::8, Encoder.qtype(:opt)::16, edns.payload_size::16, edns.rcode::8, edns.version::8,
            edns.z::16>>

        edns_data =
          case edns.data do
            %Model.EDNS.Cookie{} = cookie ->
              Model.EDNS.Cookie.encode(cookie)

            _ ->
              <<>>
          end

        <<res::bitstring, byte_size(edns_data)::16, edns_data::bitstring>>

      _ ->
        Logger.info("Got else")
        <<>>
    end
  end
end
