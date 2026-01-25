-- Table with computed columns and inline constraints
CREATE TABLE [dbo].[AuditLog] (
    [Id] UNIQUEIDENTIFIER NOT NULL DEFAULT NEWID(),
    [EntityType] NVARCHAR(100) NOT NULL,
    [EntityId] INT NOT NULL,
    [Action] NVARCHAR(50) NOT NULL CHECK ([Action] IN ('Insert', 'Update', 'Delete')),
    [OldValue] NVARCHAR(MAX) NULL,
    [NewValue] NVARCHAR(MAX) NULL,
    [UserId] INT NOT NULL,
    [Timestamp] DATETIME2 NOT NULL DEFAULT SYSDATETIME(),

    -- Computed columns
    [EntityKey] AS (CONCAT([EntityType], ':', CAST([EntityId] AS NVARCHAR(20)))),
    [HasChanges] AS (CASE WHEN [OldValue] IS NULL AND [NewValue] IS NULL THEN 0 ELSE 1 END),

    CONSTRAINT [PK_AuditLog] PRIMARY KEY NONCLUSTERED ([Id])
);
GO
