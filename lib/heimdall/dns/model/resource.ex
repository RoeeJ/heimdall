defmodule Heimdall.DNS.Model.ResourceRecord do
  alias Heimdall.DNS.{Encoder, Decoder, Model}
  import Bitwise

  @type t :: %__MODULE__{
          qname: String.t(),
          qtype: Model.qtype_atoms(),
          qclass: Model.qclass_atoms() | non_neg_integer(),
          ttl: non_neg_integer(),
          rdlength: non_neg_integer(),
          rdata: Model.EDNS.t() | bitstring() | any()
        }

  defstruct qname: nil, qtype: nil, qclass: nil, ttl: nil, rdlength: nil, rdata: nil

  def parse(records, data, 0), do: [records, data]

  def parse(records, data, count) do
    [qname, data] = Decoder.labels(data)
    <<qtype::16, data::bitstring>> = data

    case Decoder.qtype(qtype) do
      :opt ->
        <<udp_payload_size::16, extended_rcode::8, version::8, z::16, data::bitstring>> = data
        [rr, data] = Model.EDNS.parse(data)

        record = %__MODULE__{
          qname: qname,
          qtype: :opt,
          qclass: udp_payload_size,
          ttl: extended_rcode <<< 24 ||| version <<< 16 ||| z,
          rdata: rr
        }

        parse([record | records], data, count - 1)

      _ ->
        <<qclass::16, ttl::32, rdlength::16, data::bitstring>> = data
        <<rdata::binary-size(rdlength), data::bitstring>> = data

        record = %__MODULE__{
          qname: qname,
          qtype: Decoder.qtype(qtype),
          qclass: Decoder.qclass(qclass),
          ttl: ttl,
          rdlength: rdlength,
          rdata: parse_rdata(Decoder.qtype(qtype), rdata)
        }

        parse([record | records], data, count - 1)
    end
  end

  def parse_rdata(:opt, data) do
    data
  end

  def encode(%__MODULE__{qtype: :opt} = record) do
    encode_opt_record(record)
  end

  def encode(%__MODULE__{} = record) do
    encoded_name = Encoder.labels(record.qname)
    encoded_type = <<Encoder.qtype(record.qtype)::16>>
    encoded_class = <<Encoder.qclass(record.qclass)::16>>
    encoded_ttl = <<record.ttl::32>>

    {encoded_rdata, rdlength} = encode_rdata(record.qtype, record.rdata)
    encoded_rdlength = <<rdlength::16>>

    encoded_name <>
      encoded_type <> encoded_class <> encoded_ttl <> encoded_rdlength <> encoded_rdata
  end

  defp encode_opt_record(%__MODULE__{
         rdata: %Model.EDNS{} = edns,
         qclass: udp_payload_size,
         ttl: ttl
       }) do
    encoded_name = <<0>>
    encoded_type = <<Encoder.qtype(:opt)::16>>
    encoded_payload_size = <<udp_payload_size::16>>
    <<extended_rcode::8, version::8, z::16>> = <<ttl::32>>

    {encoded_option, option_length} = encode_edns_option(edns.data)
    encoded_option_length = <<option_length::16>>

    encoded_name <>
      encoded_type <>
      encoded_payload_size <>
      <<extended_rcode::8>> <>
      <<version::8>> <>
      <<z::16>> <>
      encoded_option_length <>
      encoded_option
  end

  defp encode_edns_option(%Model.EDNS.Cookie{} = cookie) do
    # 10 is the option code for Cookie
    encoded_option_code = <<10::16>>
    encoded_client_cookie = cookie.client_cookie
    encoded_server_cookie = cookie.server_cookie
    encoded_cookie_data = encoded_client_cookie <> encoded_server_cookie
    encoded_option_length = <<byte_size(encoded_cookie_data)::16>>

    encoded_option = encoded_option_code <> encoded_option_length <> encoded_cookie_data
    {encoded_option, byte_size(encoded_option)}
  end

  defp encode_edns_option(_), do: {<<>>, 0}

  defp encode_rdata(:a, {a, b, c, d}), do: {<<a::8, b::8, c::8, d::8>>, 4}

  defp encode_rdata(:aaaa, {a, b, c, d, e, f, g, h}),
    do: {<<a::16, b::16, c::16, d::16, e::16, f::16, g::16, h::16>>, 16}

  defp encode_rdata(:cname, data) when is_binary(data) do
    encoded = Encoder.labels(data)
    {encoded, byte_size(encoded)}
  end

  defp encode_rdata(:ns, data) when is_binary(data) do
    encoded = Encoder.labels(data)
    {encoded, byte_size(encoded)}
  end

  defp encode_rdata(:txt, data) when is_binary(data) do
    encoded = <<byte_size(data)::8, data::binary>>
    {encoded, byte_size(encoded)}
  end

  defp encode_rdata(:mx, {preference, exchange})
       when is_integer(preference) and is_binary(exchange) do
    encoded_exchange = Encoder.labels(exchange)
    encoded = <<preference::16, encoded_exchange::binary>>
    {encoded, byte_size(encoded)}
  end

  defp encode_rdata(:srv, {priority, weight, port, target}) when is_binary(target) do
    encoded_target = Encoder.labels(target)
    encoded = <<priority::16, weight::16, port::16, encoded_target::binary>>
    {encoded, byte_size(encoded)}
  end

  defp encode_rdata(_, data) when is_binary(data), do: {data, byte_size(data)}

  defp encode_rdata(_, data) do
    encoded = :erlang.term_to_binary(data)
    {encoded, byte_size(encoded)}
  end
end
