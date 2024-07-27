defmodule Heimdall.DNS.Decoder do
  @moduledoc """
  Decodes DNS packets.
  """
  alias Heimdall.DNS.{Model, Model.EDNS}
  import Bitwise

  @spec packet(data :: bitstring()) :: {:ok, Heimdall.DNS.Model.Packet.t()} | {:error, atom()}
  def packet(data) when is_binary(data) do
    case :inet_dns.decode(data) do
      {:ok, {:dns_rec, header, qd, an, ns, ar}} ->
        {:ok,
         %Model.Packet{
           header: decode_header(header),
           questions: decode_questions(qd),
           answers: decode_resource_records(an),
           nameservers: decode_resource_records(ns),
           additional: decode_additional(ar)
         }}

      {:error, reason} ->
        {:error, reason}
    end
  end

  defp decode_header({:dns_header, id, qr, opcode, aa, tc, rd, ra, _pr, rcode}) do
    %Model.Header{
      id: id,
      qr: qr,
      opcode: opcode,
      z: 0,
      authoritative: aa,
      truncated: tc,
      recurs_desired: rd,
      recurs_available: ra,
      resp_code: rcode(rcode)
    }
  end

  defp rcode(0), do: :noerror
  defp rcode(1), do: :formerr
  defp rcode(2), do: :servfail
  defp rcode(3), do: :nxdomain
  defp rcode(4), do: :notimp
  defp rcode(5), do: :refused
  defp rcode(6), do: :yxdomain
  defp rcode(7), do: :yxrrset
  defp rcode(8), do: :nxrrset
  defp rcode(9), do: :notauth
  defp rcode(10), do: :notzone

  defp decode_questions(questions) do
    Enum.map(questions, fn {:dns_query, qname, qtype, qclass, _} ->
      %Model.Question{qname: to_string(qname), qtype: qtype, qclass: qclass}
    end)
  end

  defp decode_resource_records(records) do
    Enum.map(records, fn rec ->
      case rec do
        {:dns_resource, qname, qtype, qclass, ttl, rdlength, data} ->
          %Model.ResourceRecord{
            qname: to_string(qname),
            qtype: qtype,
            qclass: qclass,
            ttl: ttl,
            rdlength: rdlength,
            rdata: data
          }

        {:dns_rr_opt, ~c".", :opt, udp_payload_size, ext_rcode, version, z, data, _} ->
          %Model.ResourceRecord{
            qname: "",
            qtype: :opt,
            qclass: udp_payload_size,
            ttl: ext_rcode <<< 24 ||| version <<< 16 ||| z,
            rdlength: byte_size(data),
            rdata: %EDNS{
              version: version,
              dnssec_ok: (z &&& 0x8000) != 0,
              options: decode_edns_options(data)
            }
          }
      end
    end)
  end

  defp decode_additional(records) do
    Enum.map(records, fn
      {:dns_resource, "", :opt, udp_payload_size, ttl, rdlength, data} ->
        <<_ext_rcode::8, version::8, z::16, _rest::binary>> = <<ttl::32>>

        %Model.ResourceRecord{
          qname: "",
          qtype: :opt,
          qclass: udp_payload_size,
          ttl: ttl,
          rdlength: rdlength,
          rdata: %EDNS{
            version: version,
            dnssec_ok: (z &&& 0x8000) != 0,
            options: decode_edns_options(data)
          }
        }

      record ->
        decode_resource_records([record]) |> hd()
    end)
  end

  defp decode_edns_options(<<>>), do: []

  defp decode_edns_options(<<code::16, len::16, value::binary-size(len), rest::binary>>) do
    option =
      case code do
        10 -> {:cookie, value}
        _ -> {code, value}
      end

    [option | decode_edns_options(rest)]
  end
end
