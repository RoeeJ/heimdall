defmodule Heimdall.Repo.Migrations.Initial do
  use Ecto.Migration

  def change do
    create table(:zones) do
      add :name, :string, null: false
      add :serial, :integer, null: false

      timestamps()
    end

    create table(:soa_records) do
      add :mname, :string, null: false
      add :rname, :string, null: false
      add :zone_id, references(:zones, on_delete: :delete_all), null: false

      timestamps()
    end

    create table(:dns_records) do
      add :name, :string, null: false
      add :type, :string, null: false
      add :ttl, :integer
      add :data, :map, null: false
      add :zone_id, references(:zones, on_delete: :delete_all), null: false

      timestamps()
    end

    create unique_index(:zones, [:name])
    create index(:soa_records, [:zone_id])
    create index(:dns_records, [:zone_id])
    create index(:dns_records, [:name, :type])
  end
end
