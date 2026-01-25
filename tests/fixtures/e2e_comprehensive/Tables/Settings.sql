-- Table with named inline default constraints (CONSTRAINT [name] DEFAULT syntax)
CREATE TABLE [dbo].[Settings] (
    [Id] INT NOT NULL IDENTITY(1, 1),
    [Key] NVARCHAR(100) NOT NULL,
    [Value] NVARCHAR(MAX) NULL,
    [Version] INT CONSTRAINT [DF_Settings_Version] NOT NULL DEFAULT ((0)),
    [IsActive] BIT CONSTRAINT [DF_Settings_IsActive] NOT NULL DEFAULT 1,
    [CreatedAt] DATETIME2 CONSTRAINT [DF_Settings_CreatedAt] NOT NULL DEFAULT SYSDATETIME(),
    [ModifiedAt] DATETIME2 CONSTRAINT [DF_Settings_ModifiedAt] NOT NULL DEFAULT SYSDATETIME(),
    [CreatedBy] NVARCHAR(100) CONSTRAINT [DF_Settings_CreatedBy] NOT NULL DEFAULT SYSTEM_USER,

    CONSTRAINT [PK_Settings] PRIMARY KEY CLUSTERED ([Id]),
    CONSTRAINT [UQ_Settings_Key] UNIQUE ([Key])
);
GO
