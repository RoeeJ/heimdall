defmodule Heimdall.Schema.Record do
  alias Heimdall.Schema.Zone

  @moduledoc """
  Record schema.
  """

  @derive {Jason.Encoder, except: [:__meta__, :zone]}

  @type t() :: %__MODULE__{
          id: non_neg_integer(),
          name: String.t(),
          type: Heimdall.DNS.Model.qtype_atoms(),
          ttl: non_neg_integer(),
          data: map(),
          inserted_at: DateTime.t(),
          updated_at: DateTime.t(),
          zone_id: non_neg_integer(),
          zone: Zone.t()
        }
  use Ecto.Schema
  import Ecto.Changeset

  schema "dns_records" do
    field :name, :string

    field :type, Ecto.Enum,
      values: [
        :a,
        :ns,
        :cname,
        :soa,
        :ptr,
        :hinfo,
        :mx,
        :txt,
        :rp,
        :afsdb,
        :sig,
        :key,
        :aaaa,
        :loc,
        :srv,
        :naptr,
        :kx,
        :cert,
        :dname,
        :opt,
        :apl,
        :ds,
        :sshfp,
        :ipseckey,
        :rrsig,
        :nsec,
        :dnskey,
        :dhcid,
        :nsec3,
        :nsec3param,
        :tlsa,
        :smimea,
        :hip,
        :cds,
        :cdnskey,
        :openpgpkey,
        :csync,
        :zonemd,
        :svcb,
        :https,
        :eui48,
        :eui64,
        :tkey,
        :tsig,
        :axfr,
        :mailb,
        :maila,
        :all,
        :uri,
        :caa,
        :ta,
        :dlv
      ]

    field :ttl, :integer
    field :data, :map
    belongs_to :zone, Heimdall.Schema.Zone

    timestamps()
  end

  def changeset(record, attrs) do
    record
    |> cast(attrs, [:name, :type, :zone_id, :ttl, :data])
    |> update_change(:data, &fixup_data/1)
    |> validate_required([:name, :type, :data, :zone_id])
    |> validate_name()
    |> validate_data()
  end

  @spec fixup_data(map()) :: map()
  defp fixup_data(data) do
    data
    |> Enum.map(fn
      {"preference", pref} when is_binary(pref) -> {"preference", String.to_integer(pref)}
      {"port", port} when is_binary(port) -> {"port", String.to_integer(port)}
      {"weight", weight} when is_binary(weight) -> {"weight", String.to_integer(weight)}
      {"priority", priority} when is_binary(priority) -> {"priority", String.to_integer(priority)}
      {"ttl", ttl} when is_binary(ttl) -> {"ttl", String.to_integer(ttl)}
      {key, value} -> {key, value}
    end)
    |> Enum.into(%{})
  end

  defp validate_data(changeset) do
    case get_field(changeset, :data) do
      %{"ip" => ip} -> validate_ip(changeset, ip)
      _ -> changeset
    end
  end

  defp validate_ip(changeset, ip) do
    case :inet.parse_address(String.to_charlist(ip)) do
      {:ok, _} -> changeset
      {:error, _} -> add_error(changeset, :data, "must be a valid IP address")
    end
  end

  defp validate_name(changeset) do
    case get_field(changeset, :name) do
      "" -> changeset
      name when is_binary(name) -> changeset
      _ -> add_error(changeset, :name, "must be a string or @")
    end
  end
end
