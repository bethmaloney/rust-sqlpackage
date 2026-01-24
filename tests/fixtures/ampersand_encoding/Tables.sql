-- Table with ampersand in name (edge case)
CREATE TABLE [dbo].[Terms&Conditions] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Content] NVARCHAR(MAX) NOT NULL
);
GO

-- Table with special characters
CREATE TABLE [dbo].[P&L_Report] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Revenue] DECIMAL(18, 2) NOT NULL,
    [Expenses] DECIMAL(18, 2) NOT NULL
);
GO

-- Regular table for reference
CREATE TABLE [dbo].[NormalTable] (
    [Id] INT NOT NULL PRIMARY KEY
);
GO
