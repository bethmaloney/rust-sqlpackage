-- Table with PERSISTED computed columns
CREATE TABLE [dbo].[Employees] (
    [Id] INT NOT NULL,
    [FirstName] NVARCHAR(50) NOT NULL,
    [LastName] NVARCHAR(50) NOT NULL,
    [BirthDate] DATE NOT NULL,
    [HireDate] DATE NOT NULL,
    [Salary] DECIMAL(18,2) NOT NULL,
    [BonusPercent] DECIMAL(5,2) NOT NULL DEFAULT 0,

    -- Persisted computed columns (stored physically)
    [FullName] AS ([FirstName] + N' ' + [LastName]) PERSISTED,
    [YearsEmployed] AS (DATEDIFF(YEAR, [HireDate], GETDATE())) PERSISTED,
    [TotalCompensation] AS ([Salary] * (1 + [BonusPercent] / 100)) PERSISTED,

    CONSTRAINT [PK_Employees] PRIMARY KEY ([Id])
);
GO
