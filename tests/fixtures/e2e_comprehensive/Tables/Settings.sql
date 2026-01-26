-- Table with named inline default constraints
-- Note: To name a DEFAULT constraint, the CONSTRAINT keyword must come AFTER NOT NULL
-- i.e., "NOT NULL CONSTRAINT [name] DEFAULT" names the default, not "CONSTRAINT [name] NOT NULL DEFAULT"
CREATE TABLE [dbo].[Settings] (
    [Id] INT NOT NULL IDENTITY(1, 1),
    [Key] NVARCHAR(100) NOT NULL,
    [Value] NVARCHAR(MAX) NULL,
    [Version] INT NOT NULL CONSTRAINT [DF_Settings_Version] DEFAULT ((0)),
    [IsActive] BIT NOT NULL CONSTRAINT [DF_Settings_IsActive] DEFAULT 1,
    [CreatedAt] DATETIME2 NOT NULL CONSTRAINT [DF_Settings_CreatedAt] DEFAULT SYSDATETIME(),
    [ModifiedAt] DATETIME2 NOT NULL CONSTRAINT [DF_Settings_ModifiedAt] DEFAULT SYSDATETIME(),
    [CreatedBy] NVARCHAR(100) NOT NULL CONSTRAINT [DF_Settings_CreatedBy] DEFAULT SYSTEM_USER,

    CONSTRAINT [PK_Settings] PRIMARY KEY CLUSTERED ([Id]),
    CONSTRAINT [UQ_Settings_Key] UNIQUE ([Key])
);
GO
