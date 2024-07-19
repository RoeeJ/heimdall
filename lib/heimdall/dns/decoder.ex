defmodule Heimdall.DNS.Decoder do
  alias Heimdall.DNS.{Model, Model.EDNS}
  import Bitwise

  @spec packet(data :: bitstring()) :: Heimdall.DNS.Model.Packet.t()
  def packet(data) when is_binary(data) do
    {header, rest} = decode_header(data)
    {questions, rest} = decode_questions(rest, header.qdcount)
    {answers, rest} = decode_resource_records(rest, header.ancount)
    {authority, rest} = decode_resource_records(rest, header.nscount)
    {additional, _rest} = decode_resource_records(rest, header.arcount)

    %Model.Packet{
      id: header.id,
      qr: header.qr,
      z: header.z,
      opcode: header.opcode,
      authoritative: header.aa,
      truncated: header.tc,
      recurs_desired: header.rd,
      recurs_available: header.ra,
      resp_code: header.rcode,
      questions: questions,
      answers: answers,
      nameservers: authority,
      additional: additional,
      qdcount: header.qdcount,
      ancount: header.ancount,
      nscount: header.nscount,
      arcount: header.arcount
    }
  end

  defp decode_header(
         <<id::16, flags::16, qdcount::16, ancount::16, nscount::16, arcount::16, rest::binary>>
       ) do
    <<qr::1, opcode::4, aa::1, tc::1, rd::1, ra::1, z::3, rcode::4>> = <<flags::16>>

    header = %{
      id: id,
      qr: qr(qr),
      opcode: opcode(opcode),
      z: z,
      aa: aa == 1,
      tc: tc == 1,
      rd: rd == 1,
      ra: ra == 1,
      rcode: rcode(rcode),
      qdcount: qdcount,
      ancount: ancount,
      nscount: nscount,
      arcount: arcount
    }

    {header, rest}
  end

  defp decode_questions(data, count), do: decode_questions(data, count, [])
  defp decode_questions(data, 0, acc), do: {Enum.reverse(acc), data}

  defp decode_questions(data, count, acc) do
    {question, rest} = decode_question(data)
    decode_questions(rest, count - 1, [question | acc])
  end

  defp decode_question(data) do
    {name, rest} = labels(data)
    <<qtype::16, qclass::16, rest::binary>> = rest

    question = %Model.Question{
      qname: name,
      qtype: qtype(qtype),
      qclass: qclass(qclass)
    }

    {question, rest}
  end

  defp decode_resource_records(data, count), do: decode_resource_records(data, count, [])
  defp decode_resource_records(data, 0, acc), do: {Enum.reverse(acc), data}

  defp decode_resource_records(data, count, acc) do
    {rr, rest} = decode_resource_record(data)
    decode_resource_records(rest, count - 1, [rr | acc])
  end

  defp decode_resource_record(data) do
    {qname, data} = labels(data, [])
    <<qtype::16, qclass::16, ttl::32, rdlength::16, data::bitstring>> = data
    <<rdata::binary-size(rdlength), rest::bitstring>> = data

    record =
      if qtype(qtype) == :opt do
        [edns_data, _] = EDNS.parse(<<qclass::16, ttl::32, rdlength::16, rdata::binary>>)

        %Model.ResourceRecord{
          qname: qname,
          qtype: :opt,
          qclass: edns_data.payload_size,
          ttl: edns_data.rcode <<< 24 ||| edns_data.version <<< 16 ||| edns_data.z,
          rdlength: rdlength,
          rdata: edns_data
        }
      else
        %Model.ResourceRecord{
          qname: qname,
          qtype: qtype(qtype),
          qclass: qclass(qclass),
          ttl: ttl,
          rdlength: rdlength,
          rdata: decode_rdata(qtype(qtype), rdata)
        }
      end

    {record, rest}
  end

  def labels(data, acc \\ [])
  def labels(<<0, rest::binary>>, acc) do
    label = if acc == [], do: ".", else: Enum.reverse(acc) |> Enum.join(".")
    {label, rest}
  end

  def labels(<<len, part::binary-size(len), rest::binary>>, acc) do
    labels(rest, [to_string(part) | acc])
  end

  defp decode_rdata(:a, <<a, b, c, d>>), do: {a, b, c, d}

  defp decode_rdata(:aaaa, <<a::16, b::16, c::16, d::16, e::16, f::16, g::16, h::16>>),
    do: {a, b, c, d, e, f, g, h}

  defp decode_rdata(:cname, data), do: labels(data) |> elem(0)
  defp decode_rdata(:txt, <<length::8, text::binary-size(length)>>), do: to_string(text)

  defp decode_rdata(:mx, <<preference::16, exchange::binary>>),
    do: {preference, labels(exchange) |> elem(0)}

  defp decode_rdata(:srv, <<priority::16, weight::16, port::16, target::binary>>) do
    {priority, weight, port, labels(target) |> elem(0)}
  end

  defp decode_rdata(:opt, data), do: Model.EDNS.parse(data)
  defp decode_rdata(_, data), do: data

  def qr(0), do: :query
  def qr(1), do: :response
  def qr(n), do: throw("Unknown qr value: #{n}")

  @spec opcode(Model.opcode_ints()) :: Model.opcode_atoms()
  def opcode(0), do: :query
  def opcode(1), do: :iquery
  def opcode(2), do: :status
  def opcode(n), do: throw("Unknown opcode value: #{n}")

  @spec rcode(0 | 1 | 2 | 3 | 4 | 5) ::
          :format_err | :name_err | :no_err | :not_implemented | :refused | :server_err
  def rcode(0), do: :no_err
  def rcode(1), do: :server_err
  def rcode(2), do: :format_err
  def rcode(3), do: :name_err
  def rcode(4), do: :not_implemented
  def rcode(5), do: :refused
  def rcode(n), do: throw("Unknown rcode value: #{n}")

  @spec qtype(1 | 2 | 5 | 6 | 12 | 15 | 16 | 28 | 33 | 41) ::
          :a | :aaaa | :cname | :mx | :ns | :opt | :ptr | :soa | :srv | :txt
  def qtype(1), do: :a
  def qtype(2), do: :ns
  def qtype(5), do: :cname
  def qtype(6), do: :soa
  def qtype(12), do: :ptr
  def qtype(15), do: :mx
  def qtype(16), do: :txt
  def qtype(28), do: :aaaa
  def qtype(33), do: :srv
  def qtype(41), do: :opt
  def qtype(n), do: throw("Unknown qtype value: #{n}")

  @spec qclass(1 | 2 | 3 | 4) :: :ch | :cs | :hs | :in
  def qclass(1), do: :in
  def qclass(2), do: :cs
  def qclass(3), do: :ch
  def qclass(4), do: :hs
  def qclass(n), do: throw("Unknown qclass value: #{n}")
end
